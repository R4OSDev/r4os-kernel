const std = @import("std");
const r4x_api = @import("../program/r4x_api.zig");
const monotonic = @import("../platform/monotonic.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const task_context = @import("../sched/task_context.zig");
const timer = @import("timer.zig");

pub const MAX_SERVICES: usize = 16;
pub const MAX_NAME: usize = 32;
pub const MAX_PATH: usize = 1024; // contract file_path_max_bytes + NUL (0.60.19)
pub const MAX_ARGS: usize = 96;
pub const MAX_DESCRIPTION: usize = 80;
pub const MAX_ERROR: usize = 64;

pub const OK: i32 = 0;
pub const ERR_INVALID: i32 = -1;
pub const ERR_FULL: i32 = -2;
pub const ERR_DUPLICATE: i32 = -3;
pub const ERR_NOT_FOUND: i32 = -4;

pub const API_MAGIC: u32 = 0x43565352; // "RSVC" little endian
pub const API_VERSION: u16 = 1;
pub const API_HEADER_SIZE: usize = 28;
pub const API_MAX_PAYLOAD: usize = 4096;
pub const API_ENDPOINT_QUEUE_DEPTH: usize = 8;
pub const API_OK: i32 = 0;
pub const API_ERR_INVALID: i32 = -1;
pub const API_ERR_NOT_FOUND: i32 = -2;
pub const API_ERR_NOT_RUNNING: i32 = -3;
pub const API_ERR_NO_ENDPOINT: i32 = -4;
pub const API_ERR_PAYLOAD_TOO_LARGE: i32 = -5;
pub const API_ERR_BUFFER_TOO_SMALL: i32 = -6;
pub const API_ERR_BUSY: i32 = -7;
pub const API_ERR_TIMEOUT: i32 = -8;
pub const API_ERR_BAD_HANDLE: i32 = -9;
pub const API_ERR_FULL: i32 = -10;
pub const API_ERR_BAD_OP: i32 = -11;
pub const API_ERR_DUPLICATE: i32 = -12;
pub const API_ERR_BAD_PATH: i32 = -13;
pub const API_ERR_CONFIG_IO: i32 = -14;
pub const API_ERR_RUNNING: i32 = -15;
pub const API_ERR_DISABLED: i32 = -16;
pub const API_ERR_SPAWN_FAILED: i32 = -17;
pub const API_ERR_STOP_FAILED: i32 = -18;
/// The caller runs inside the very service it asked to stop or restart
/// (0.60.29).  Carrying that out would kill the caller's own tree mid-syscall
/// and report a success nobody is left to observe, so it is refused visibly
/// instead.  Appended on purpose: existing codes keep their values.
pub const API_ERR_SELF_RESTART: i32 = -19;

pub const API_FLAG_ENDPOINT: u32 = 1 << 0;
pub const API_FLAG_REQUEST_PENDING: u32 = 1 << 1;
pub const API_FLAG_RESPONSE_PENDING: u32 = 1 << 2;
pub const API_FLAG_QUEUE_BACKED: u32 = 1 << 3;

pub const API_STATE_EMPTY: u32 = 0;
pub const API_STATE_STOPPED: u32 = 1;
pub const API_STATE_STARTING: u32 = 2;
pub const API_STATE_RUNNING: u32 = 3;
pub const API_STATE_STOPPING: u32 = 4;
pub const API_STATE_FAILED: u32 = 5;
pub const API_STATE_DISABLED: u32 = 6;

pub const API_START_MANUAL: u32 = 1;
pub const API_START_AUTO: u32 = 2;
pub const API_START_DISABLED: u32 = 3;

pub const ApiInfo = r4x_api.ServiceInfo;
pub const ApiDetail = r4x_api.ServiceDetail;
pub const ApiMessageHeader = r4x_api.ServiceMessageHeader;

const MAX_ENDPOINTS: usize = MAX_SERVICES;
const SERVICE_LOCK_FAMILY_COUNT: usize = 7;
const SERVICE_LOCK_TIMING_STRIDE: u64 = 64;
const REQUEST_ID_BLOCK_SIZE: u32 = 256;
const REQUEST_ID_MAX: u32 = 0x7FFF_FFFE;
const API_INDEX_INVALID_SLOT: u8 = 0xFF;

const LockFamily = enum(u8) {
    registry_control = 0,
    registry_lookup = 1,
    registry_snapshot = 2,
    endpoint_lifecycle = 3,
    endpoint_data = 4,
    endpoint_wait = 5,
    endpoint_snapshot = 6,
};

const LockTiming = struct {
    acquisitions: u64 = 0,
    contentions: u64 = 0,
    timing_samples: u64 = 0,
    wait_ns: u64 = 0,
    wait_max_ns: u64 = 0,
    hold_ns: u64 = 0,
    hold_max_ns: u64 = 0,
    timing_unavailable: u64 = 0,
};

const TimedLock = struct {
    mutex: *sync.Mutex,
    timing: ?*LockTiming = null,
    hold_started: monotonic.Stamp = .{},
    timed: bool = false,
    locked: bool = false,
    admitted: bool = false,
};

const EndpointLifetimePerformance = struct {
    payload_copy_bytes: u64 = 0,
    payload_clear_bytes: u64 = 0,
    slot_metadata_resets: u64 = 0,
    endpoint_metadata_resets: u64 = 0,
    endpoint_payload_reset_bytes: u64 = 0,
    queue_scan_passes: u64 = 0,
    queue_scan_slots: u64 = 0,
    revalidations: u64 = 0,
    stale_rejections: u64 = 0,
};

const RequestState = enum(u8) {
    free,
    queued,
    delivered,
    responded,
};

const RequestSlot = struct {
    state: RequestState = .free,
    request_id: u32 = 0,
    client_id: u32 = 0,
    op: u16 = 0,
    flags: u32 = 0,
    request_len: u16 = 0,
    response_len: u16 = 0,
    response_status: i32 = API_OK,
    response_available: sync.WaitQueue = sync.WaitQueue.init(),
    request_payload: [API_MAX_PAYLOAD]u8 = .{0} ** API_MAX_PAYLOAD,
    response_payload: [API_MAX_PAYLOAD]u8 = .{0} ** API_MAX_PAYLOAD,
};

pub const State = enum(u8) {
    empty,
    stopped,
    starting,
    running,
    stopping,
    failed,
    disabled,
};

pub const StartMode = enum(u8) {
    manual,
    auto,
    disabled,
};

pub const Entry = struct {
    used: bool = false,
    state: State = .empty,
    start_mode: StartMode = .manual,
    instance_id: u32 = 0,
    exit_code: i32 = 0,
    start_tick: u64 = 0,
    restart_count: u32 = 0,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    path: [MAX_PATH]u8 = .{0} ** MAX_PATH,
    path_len: usize = 0,
    args: [MAX_ARGS]u8 = .{0} ** MAX_ARGS,
    args_len: usize = 0,
    description: [MAX_DESCRIPTION]u8 = .{0} ** MAX_DESCRIPTION,
    description_len: usize = 0,
    last_error: [MAX_ERROR]u8 = .{0} ** MAX_ERROR,
    last_error_len: usize = 0,
};

/// Stable internal identity for one dense ServiceInfo/ServiceDetail index.
/// Structural registry changes invalidate every outstanding target through
/// registry_generation; ordinary state changes keep the target usable as
/// long as the same service instance remains attached.
pub const ApiIndexTarget = struct {
    registry_generation: u64 = 0,
    api_index: u32 = 0,
    service_slot: usize = 0,
    instance_id: u32 = 0,
    state: State = .empty,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
};

const Endpoint = struct {
    // The lock, generation, reserved request-ID range and lifetime counters
    // are stable slot identity.
    // resetEndpointMetadata must never replace them while stale waiters may
    // still be resuming against this address.
    lock: sync.Mutex = sync.Mutex.initClass("service-endpoint", sync.LockRank.service_endpoint, .sleepable),
    generation: u64 = 0,
    request_id_next: u32 = 0,
    request_id_limit: u32 = 0,
    lifetime: EndpointLifetimePerformance = .{},
    lock_timing: [SERVICE_LOCK_FAMILY_COUNT]LockTiming = .{LockTiming{}} ** SERVICE_LOCK_FAMILY_COUNT,
    used: bool = false,
    service_slot: usize = 0,
    handle: u32 = 0,
    flags: u32 = 0,
    queue_high_water: u32 = 0,
    max_active_workers: u32 = 0,
    open_handles: u32 = 0,
    requests: u64 = 0,
    responses: u64 = 0,
    drops: u64 = 0,
    busy_rejections: u64 = 0,
    timeouts: u64 = 0,
    cancellations: u64 = 0,
    completion_waits: u64 = 0,
    completion_timeouts: u64 = 0,
    completion_wait_rounds: u64 = 0,
    targeted_response_wakes: u64 = 0,
    targeted_response_wake_misses: u64 = 0,
    admission_waits: u64 = 0,
    admission_timeouts: u64 = 0,
    requests_available: sync.WaitQueue = sync.WaitQueue.init(),
    slots_available: sync.Semaphore = sync.Semaphore.init(@intCast(API_ENDPOINT_QUEUE_DEPTH), @intCast(API_ENDPOINT_QUEUE_DEPTH)),
    queue: [API_ENDPOINT_QUEUE_DEPTH]RequestSlot = .{RequestSlot{}} ** API_ENDPOINT_QUEUE_DEPTH,
};

pub const PerformanceSummary = struct {
    max_services: u32 = @intCast(MAX_SERVICES),
    used_services: u32 = 0,
    running_services: u32 = 0,
    endpoints_used: u32 = 0,
    request_pending: u32 = 0,
    response_pending: u32 = 0,
    queue_depth_total: u32 = 0,
    queue_used_total: u32 = 0,
    queue_high_water_total: u32 = 0,
    active_workers: u32 = 0,
    max_active_workers: u32 = 0,
    open_handles: u32 = 0,
    requests: u64 = 0,
    responses: u64 = 0,
    drops: u64 = 0,
    busy_rejections: u64 = 0,
    timeouts: u64 = 0,
    cancellations: u64 = 0,
    completion_waits: u64 = 0,
    completion_timeouts: u64 = 0,
    completion_wait_rounds: u64 = 0,
    targeted_response_wakes: u64 = 0,
    targeted_response_wake_misses: u64 = 0,
    admission_waits: u64 = 0,
    admission_timeouts: u64 = 0,
    payload_copy_bytes: u64 = 0,
    payload_clear_bytes: u64 = 0,
    slot_metadata_resets: u64 = 0,
    endpoint_metadata_resets: u64 = 0,
    endpoint_payload_reset_bytes: u64 = 0,
    queue_scan_passes: u64 = 0,
    queue_scan_slots: u64 = 0,
    endpoint_revalidations: u64 = 0,
    endpoint_stale_rejections: u64 = 0,
    lock_family_count: u32 = @intCast(SERVICE_LOCK_FAMILY_COUNT),
    lock_reserved0: u32 = 0,
    lock_acquisitions: [SERVICE_LOCK_FAMILY_COUNT]u64 = .{0} ** SERVICE_LOCK_FAMILY_COUNT,
    lock_contentions: [SERVICE_LOCK_FAMILY_COUNT]u64 = .{0} ** SERVICE_LOCK_FAMILY_COUNT,
    lock_wait_ns: [SERVICE_LOCK_FAMILY_COUNT]u64 = .{0} ** SERVICE_LOCK_FAMILY_COUNT,
    lock_wait_max_ns: [SERVICE_LOCK_FAMILY_COUNT]u64 = .{0} ** SERVICE_LOCK_FAMILY_COUNT,
    lock_hold_ns: [SERVICE_LOCK_FAMILY_COUNT]u64 = .{0} ** SERVICE_LOCK_FAMILY_COUNT,
    lock_hold_max_ns: [SERVICE_LOCK_FAMILY_COUNT]u64 = .{0} ** SERVICE_LOCK_FAMILY_COUNT,
    lock_timing_unavailable: [SERVICE_LOCK_FAMILY_COUNT]u64 = .{0} ** SERVICE_LOCK_FAMILY_COUNT,
    lock_timing_stride: u32 = @intCast(SERVICE_LOCK_TIMING_STRIDE),
    lock_timing_reserved0: u32 = 0,
    lock_timing_samples: [SERVICE_LOCK_FAMILY_COUNT]u64 = .{0} ** SERVICE_LOCK_FAMILY_COUNT,
    registry_index_queries: u64 = 0,
    registry_refresh_requests: u64 = 0,
    registry_refresh_visits: u64 = 0,
    registry_instance_lookups: u64 = 0,
    registry_index_end_markers: u64 = 0,
};

const RegistryEnumerationPerformance = struct {
    index_queries: u64 = 0,
    refresh_requests: u64 = 0,
    refresh_visits: u64 = 0,
    instance_lookups: u64 = 0,
    index_end_markers: u64 = 0,
};

var entries: [MAX_SERVICES]Entry = .{Entry{}} ** MAX_SERVICES;
var endpoints: [MAX_ENDPOINTS]Endpoint = .{Endpoint{}} ** MAX_ENDPOINTS;
var next_endpoint_handle: u32 = 1;
var next_request_id: u32 = 1;
var registry_lock = sync.Mutex.initClass("service-registry", sync.LockRank.service_registry, .sleepable);
var registry_lock_timing: [SERVICE_LOCK_FAMILY_COUNT]LockTiming = .{LockTiming{}} ** SERVICE_LOCK_FAMILY_COUNT;
var api_index_slots: [MAX_SERVICES]u8 = .{API_INDEX_INVALID_SLOT} ** MAX_SERVICES;
var api_index_count: u32 = 0;
var registry_generation: u64 = 1;
var registry_enumeration_performance: RegistryEnumerationPerformance = .{};

fn lockFamilyIndex(family: LockFamily) usize {
    return @intFromEnum(family);
}

fn noteElapsed(timing: *LockTiming, start: monotonic.Stamp, end: monotonic.Stamp, is_hold: bool) void {
    const elapsed = monotonic.elapsedNanoseconds(start, end) orelse {
        timing.timing_unavailable +%= 1;
        return;
    };
    if (is_hold) {
        timing.hold_ns +%= elapsed;
        if (elapsed > timing.hold_max_ns) timing.hold_max_ns = elapsed;
    } else {
        timing.wait_ns +%= elapsed;
        if (elapsed > timing.wait_max_ns) timing.wait_max_ns = elapsed;
    }
}

fn acquireTimedLock(mutex: *sync.Mutex, timing: *LockTiming) TimedLock {
    // Before scheduler admission the kernel is single-threaded. Preserve the
    // historical boot-time lock bypass without pretending it was measured.
    if (scheduler.current() == null) return .{ .mutex = mutex, .admitted = true };

    const immediate = mutex.tryLock();
    const wait_started = if (immediate) monotonic.Stamp{} else monotonic.capture();
    const acquired = immediate or mutex.lock(sync.WAIT_FOREVER);
    if (!acquired) return .{ .mutex = mutex };
    timing.acquisitions +%= 1;
    if (!immediate) timing.contentions +%= 1;
    const sampled = !immediate or ((timing.acquisitions -% 1) % SERVICE_LOCK_TIMING_STRIDE == 0);
    const acquired_at = if (sampled) monotonic.capture() else monotonic.Stamp{};
    if (sampled) {
        timing.timing_samples +%= 1;
        // An uncontended try-lock has no scheduler wait. Measuring its few
        // instructions with HPET MMIO would perturb the hot path more than
        // the lock itself, so only real contention contributes wait time.
        if (!immediate) noteElapsed(timing, wait_started, acquired_at, false);
    }
    return .{
        .mutex = mutex,
        .timing = timing,
        .hold_started = acquired_at,
        .timed = sampled,
        .locked = true,
        .admitted = true,
    };
}

fn tryAcquireTimedLock(mutex: *sync.Mutex, timing: *LockTiming) ?TimedLock {
    if (scheduler.current() == null) return TimedLock{ .mutex = mutex, .admitted = true };
    if (!mutex.tryLock()) {
        timing.contentions +%= 1;
        return null;
    }
    timing.acquisitions +%= 1;
    const sampled = (timing.acquisitions -% 1) % SERVICE_LOCK_TIMING_STRIDE == 0;
    const acquired_at = if (sampled) monotonic.capture() else monotonic.Stamp{};
    if (sampled) timing.timing_samples +%= 1;
    return .{
        .mutex = mutex,
        .timing = timing,
        .hold_started = acquired_at,
        .timed = sampled,
        .locked = true,
        .admitted = true,
    };
}

fn releaseTimedLock(guard: *TimedLock) void {
    if (guard.locked) {
        if (guard.timed) {
            if (guard.timing) |timing| noteElapsed(timing, guard.hold_started, monotonic.capture(), true);
        }
        _ = guard.mutex.unlock();
    }
    guard.locked = false;
    guard.admitted = false;
}

fn releaseTimedLockForWait(raw: *anyopaque) void {
    const guard: *TimedLock = @ptrCast(@alignCast(raw));
    releaseTimedLock(guard);
}

fn lockRegistry(family: LockFamily) TimedLock {
    return acquireTimedLock(&registry_lock, &registry_lock_timing[lockFamilyIndex(family)]);
}

fn unlockRegistry(guard: *TimedLock) void {
    releaseTimedLock(guard);
}

fn rebuildApiIndexLocked() void {
    @memset(api_index_slots[0..], API_INDEX_INVALID_SLOT);
    api_index_count = 0;
    var slot: usize = 0;
    while (slot < entries.len) : (slot += 1) {
        if (!entries[slot].used) continue;
        api_index_slots[api_index_count] = @intCast(slot);
        api_index_count += 1;
    }
}

fn noteRegistryStructureChangedLocked() void {
    registry_generation +%= 1;
    if (registry_generation == 0) registry_generation = 1;
    rebuildApiIndexLocked();
}

fn apiSlotAtLocked(index: u32) ?usize {
    if (index >= api_index_count) return null;
    const slot: usize = api_index_slots[index];
    if (slot >= entries.len or !entries[slot].used) return null;
    return slot;
}

fn apiTargetSlotLocked(target: ApiIndexTarget, require_instance: bool) ?usize {
    if (target.registry_generation != registry_generation) return null;
    const slot = apiSlotAtLocked(target.api_index) orelse return null;
    if (slot != target.service_slot) return null;
    const entry = &entries[slot];
    if (entry.name_len != target.name_len or
        !nameEq(entry.name[0..entry.name_len], target.name[0..target.name_len])) return null;
    if (require_instance and entry.instance_id != target.instance_id) return null;
    return slot;
}

fn selectApiIndexTarget(index: u32, count_query: bool) ?ApiIndexTarget {
    var registry_guard = lockRegistry(.registry_snapshot);
    defer unlockRegistry(&registry_guard);
    if (count_query) registry_enumeration_performance.index_queries +%= 1;
    const slot = apiSlotAtLocked(index) orelse {
        if (count_query) registry_enumeration_performance.index_end_markers +%= 1;
        return null;
    };
    const entry = &entries[slot];
    if (count_query) registry_enumeration_performance.refresh_requests +%= 1;
    registry_enumeration_performance.refresh_visits +%= 1;
    var target = ApiIndexTarget{
        .registry_generation = registry_generation,
        .api_index = index,
        .service_slot = slot,
        .instance_id = entry.instance_id,
        .state = entry.state,
        .name_len = entry.name_len,
    };
    if (entry.name_len > 0) @memcpy(target.name[0..entry.name_len], entry.name[0..entry.name_len]);
    return target;
}

pub fn beginApiIndexRefresh(index: u32) ?ApiIndexTarget {
    return selectApiIndexTarget(index, true);
}

pub fn retryApiIndexRefresh(index: u32) ?ApiIndexTarget {
    return selectApiIndexTarget(index, false);
}

pub fn entryForApiIndexTarget(target: ApiIndexTarget) ?Entry {
    var registry_guard = lockRegistry(.registry_snapshot);
    defer unlockRegistry(&registry_guard);
    const slot = apiTargetSlotLocked(target, true) orelse return null;
    return entries[slot];
}

pub fn noteApiIndexInstanceLookup(target: ApiIndexTarget) bool {
    var registry_guard = lockRegistry(.registry_snapshot);
    defer unlockRegistry(&registry_guard);
    if (apiTargetSlotLocked(target, true) == null) return false;
    registry_enumeration_performance.instance_lookups +%= 1;
    return true;
}

const EndpointIdentity = struct {
    endpoint: *Endpoint,
    handle: u32,
    generation: u64,
    service_slot: usize,
};

const EndpointLease = struct {
    endpoint: *Endpoint,
    identity: EndpointIdentity,
    guard: TimedLock,
};

const LifecycleEndpointLease = struct {
    endpoint: *Endpoint,
    guard: TimedLock,
};

const LifecycleAttempt = union(enum) {
    none,
    contended: *Endpoint,
    locked: LifecycleEndpointLease,
};

const QueueCounts = struct {
    queued: u32 = 0,
    delivered: u32 = 0,
    responded: u32 = 0,
    used: u32 = 0,
};

fn addLockTiming(out: *PerformanceSummary, index: usize, timing: LockTiming) void {
    out.lock_acquisitions[index] +%= timing.acquisitions;
    out.lock_contentions[index] +%= timing.contentions;
    out.lock_wait_ns[index] +%= timing.wait_ns;
    if (timing.wait_max_ns > out.lock_wait_max_ns[index]) out.lock_wait_max_ns[index] = timing.wait_max_ns;
    out.lock_hold_ns[index] +%= timing.hold_ns;
    if (timing.hold_max_ns > out.lock_hold_max_ns[index]) out.lock_hold_max_ns[index] = timing.hold_max_ns;
    out.lock_timing_unavailable[index] +%= timing.timing_unavailable;
    out.lock_timing_samples[index] +%= timing.timing_samples;
}

fn addEndpointLifetime(out: *PerformanceSummary, ep: *const Endpoint) void {
    out.payload_copy_bytes +%= ep.lifetime.payload_copy_bytes;
    out.payload_clear_bytes +%= ep.lifetime.payload_clear_bytes;
    out.slot_metadata_resets +%= ep.lifetime.slot_metadata_resets;
    out.endpoint_metadata_resets +%= ep.lifetime.endpoint_metadata_resets;
    out.endpoint_payload_reset_bytes +%= ep.lifetime.endpoint_payload_reset_bytes;
    out.queue_scan_passes +%= ep.lifetime.queue_scan_passes;
    out.queue_scan_slots +%= ep.lifetime.queue_scan_slots;
    out.endpoint_revalidations +%= ep.lifetime.revalidations;
    out.endpoint_stale_rejections +%= ep.lifetime.stale_rejections;
}

fn tryLifecycleEndpoint(ep: *Endpoint) LifecycleAttempt {
    const timing = &ep.lock_timing[lockFamilyIndex(.endpoint_lifecycle)];
    const guard = tryAcquireTimedLock(&ep.lock, timing) orelse return .{ .contended = ep };
    return .{ .locked = .{ .endpoint = ep, .guard = guard } };
}

fn tryLifecycleEndpointForSlot(slot: usize) LifecycleAttempt {
    const index = endpointForSlot(slot) orelse return .none;
    return tryLifecycleEndpoint(&endpoints[index]);
}

fn tryLifecycleEndpointForHandle(handle: u32) LifecycleAttempt {
    const index = endpointForHandle(handle) orelse return .none;
    return tryLifecycleEndpoint(&endpoints[index]);
}

fn waitForEndpointLifecycle(ep: *Endpoint) void {
    var guard = acquireTimedLock(&ep.lock, &ep.lock_timing[lockFamilyIndex(.endpoint_lifecycle)]);
    if (guard.admitted) releaseTimedLock(&guard);
}

fn endpointIdentityForSlotLocked(slot: usize) ?EndpointIdentity {
    const index = endpointForSlot(slot) orelse return null;
    const ep = &endpoints[index];
    return .{
        .endpoint = ep,
        .handle = ep.handle,
        .generation = ep.generation,
        .service_slot = ep.service_slot,
    };
}

fn endpointIdentityForHandleLocked(handle: u32) ?EndpointIdentity {
    const index = endpointForHandle(handle) orelse return null;
    const ep = &endpoints[index];
    return .{
        .endpoint = ep,
        .handle = ep.handle,
        .generation = ep.generation,
        .service_slot = ep.service_slot,
    };
}

fn lookupEndpointIdentity(handle: u32) ?EndpointIdentity {
    var registry_guard = lockRegistry(.registry_lookup);
    defer unlockRegistry(&registry_guard);
    return endpointIdentityForHandleLocked(handle);
}

fn identityMatches(ep: *const Endpoint, identity: EndpointIdentity) bool {
    return ep.used and
        ep.handle == identity.handle and
        ep.generation == identity.generation and
        ep.service_slot == identity.service_slot;
}

fn lockEndpointIdentity(identity: EndpointIdentity, family: LockFamily) ?EndpointLease {
    var guard = acquireTimedLock(&identity.endpoint.lock, &identity.endpoint.lock_timing[lockFamilyIndex(family)]);
    if (!guard.admitted) return null;
    identity.endpoint.lifetime.revalidations +%= 1;
    if (!identityMatches(identity.endpoint, identity)) {
        identity.endpoint.lifetime.stale_rejections +%= 1;
        releaseTimedLock(&guard);
        return null;
    }
    return .{ .endpoint = identity.endpoint, .identity = identity, .guard = guard };
}

fn lockEndpointForHandle(handle: u32, family: LockFamily) ?EndpointLease {
    const identity = lookupEndpointIdentity(handle) orelse return null;
    return lockEndpointIdentity(identity, family);
}

fn unlockEndpoint(lease: *EndpointLease) void {
    releaseTimedLock(&lease.guard);
}

fn markEntryStarting(entry: *Entry) void {
    entry.state = .starting;
    entry.instance_id = 0;
    entry.exit_code = 0;
    entry.last_error_len = 0;
}

fn disableEntry(entry: *Entry) void {
    entry.start_mode = .disabled;
    entry.state = .disabled;
    entry.instance_id = 0;
    entry.start_tick = 0;
}

fn ifEntryDisabledElseStopped(entry: *const Entry) State {
    return if (entry.start_mode == .disabled) .disabled else .stopped;
}

fn alwaysFailed(_: *const Entry) State {
    return .failed;
}

fn finishServiceState(name: []const u8, stateFor: *const fn (*const Entry) State, exit_code: i32, error_text: []const u8) i32 {
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        const slot = findByNameIn(&entries, name) orelse {
            unlockRegistry(&registry_guard);
            return ERR_NOT_FOUND;
        };
        switch (tryLifecycleEndpointForSlot(slot)) {
            .contended => |ep| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(ep);
                continue;
            },
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                retireEndpointLocked(locked.endpoint, .cancelled);
                finishEntryState(&entries[slot], stateFor, exit_code, error_text);
                releaseTimedLock(&endpoint_guard);
            },
            .none => finishEntryState(&entries[slot], stateFor, exit_code, error_text),
        }
        unlockRegistry(&registry_guard);
        return OK;
    }
}

fn finishEntryState(entry: *Entry, stateFor: *const fn (*const Entry) State, exit_code: i32, error_text: []const u8) void {
    entry.state = stateFor(entry);
    entry.instance_id = 0;
    entry.exit_code = exit_code;
    entry.start_tick = 0;
    entry.last_error_len = copy(error_text, entry.last_error[0..]);
}

pub fn init() void {
    resetRegistryState();
}

pub fn register(name: []const u8, path: []const u8, args: []const u8, start_mode: StartMode) i32 {
    return registerWithDescription(name, path, args, start_mode, "");
}

pub fn registerWithDescription(name: []const u8, path: []const u8, args: []const u8, start_mode: StartMode, description: []const u8) i32 {
    var registry_guard = lockRegistry(.registry_control);
    defer unlockRegistry(&registry_guard);
    const result = registerIn(&entries, name, path, args, start_mode, description);
    if (result >= 0) noteRegistryStructureChangedLocked();
    return result;
}

pub fn unregister(name: []const u8) i32 {
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        const slot = findByNameIn(&entries, name) orelse {
            unlockRegistry(&registry_guard);
            return ERR_NOT_FOUND;
        };
        switch (tryLifecycleEndpointForSlot(slot)) {
            .contended => |ep| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(ep);
                continue;
            },
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                retireEndpointLocked(locked.endpoint, .cancelled);
                const result = unregisterIn(&entries, name);
                if (result == OK) noteRegistryStructureChangedLocked();
                releaseTimedLock(&endpoint_guard);
                unlockRegistry(&registry_guard);
                return result;
            },
            .none => {
                const result = unregisterIn(&entries, name);
                if (result == OK) noteRegistryStructureChangedLocked();
                unlockRegistry(&registry_guard);
                return result;
            },
        }
    }
}

pub fn setState(name: []const u8, state: State, instance_id: u32, exit_code: i32, error_text: []const u8) i32 {
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        const slot = findByNameIn(&entries, name) orelse {
            unlockRegistry(&registry_guard);
            return ERR_NOT_FOUND;
        };
        const retire = state != .running or entries[slot].instance_id != instance_id;
        if (!retire) {
            const result = setStateIn(&entries, name, state, instance_id, exit_code, error_text);
            unlockRegistry(&registry_guard);
            return result;
        }
        switch (tryLifecycleEndpointForSlot(slot)) {
            .contended => |ep| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(ep);
                continue;
            },
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                retireEndpointLocked(locked.endpoint, .cancelled);
                const result = setStateIn(&entries, name, state, instance_id, exit_code, error_text);
                releaseTimedLock(&endpoint_guard);
                unlockRegistry(&registry_guard);
                return result;
            },
            .none => {
                const result = setStateIn(&entries, name, state, instance_id, exit_code, error_text);
                unlockRegistry(&registry_guard);
                return result;
            },
        }
    }
}

pub fn markStarting(name: []const u8) i32 {
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        const slot = findByNameIn(&entries, name) orelse {
            unlockRegistry(&registry_guard);
            return ERR_NOT_FOUND;
        };
        if (entries[slot].start_mode == .disabled) {
            unlockRegistry(&registry_guard);
            return ERR_INVALID;
        }
        switch (tryLifecycleEndpointForSlot(slot)) {
            .contended => |ep| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(ep);
                continue;
            },
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                retireEndpointLocked(locked.endpoint, .cancelled);
                markEntryStarting(&entries[slot]);
                releaseTimedLock(&endpoint_guard);
            },
            .none => markEntryStarting(&entries[slot]),
        }
        unlockRegistry(&registry_guard);
        return OK;
    }
}

pub fn markRunning(name: []const u8, instance_id: u32, start_tick: u64) i32 {
    var registry_guard = lockRegistry(.registry_control);
    defer unlockRegistry(&registry_guard);
    const slot = findByNameIn(&entries, name) orelse return ERR_NOT_FOUND;
    var e = &entries[slot];
    if (e.start_mode == .disabled or instance_id == 0) return ERR_INVALID;
    e.state = .running;
    e.instance_id = instance_id;
    e.exit_code = 0;
    e.start_tick = start_tick;
    e.last_error_len = 0;
    return OK;
}

pub fn markStopping(name: []const u8) i32 {
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        const slot = findByNameIn(&entries, name) orelse {
            unlockRegistry(&registry_guard);
            return ERR_NOT_FOUND;
        };
        if (entries[slot].state != .running and entries[slot].state != .starting) {
            unlockRegistry(&registry_guard);
            return ERR_INVALID;
        }
        switch (tryLifecycleEndpointForSlot(slot)) {
            .contended => |ep| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(ep);
                continue;
            },
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                retireEndpointLocked(locked.endpoint, .cancelled);
                entries[slot].state = .stopping;
                releaseTimedLock(&endpoint_guard);
            },
            .none => entries[slot].state = .stopping,
        }
        unlockRegistry(&registry_guard);
        return OK;
    }
}

pub fn markStopped(name: []const u8, exit_code: i32) i32 {
    return finishServiceState(name, ifEntryDisabledElseStopped, exit_code, "");
}

pub fn markFailed(name: []const u8, exit_code: i32, error_text: []const u8) i32 {
    return finishServiceState(name, alwaysFailed, exit_code, error_text);
}

pub fn markStoppingTarget(target: ApiIndexTarget) i32 {
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        const slot = apiTargetSlotLocked(target, true) orelse {
            unlockRegistry(&registry_guard);
            return ERR_NOT_FOUND;
        };
        if (entries[slot].state != .running and entries[slot].state != .starting) {
            unlockRegistry(&registry_guard);
            return ERR_INVALID;
        }
        switch (tryLifecycleEndpointForSlot(slot)) {
            .contended => |ep| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(ep);
                continue;
            },
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                retireEndpointLocked(locked.endpoint, .cancelled);
                entries[slot].state = .stopping;
                releaseTimedLock(&endpoint_guard);
            },
            .none => entries[slot].state = .stopping,
        }
        unlockRegistry(&registry_guard);
        return OK;
    }
}

pub fn markStoppedTarget(target: ApiIndexTarget, exit_code: i32) i32 {
    return finishServiceTargetState(target, ifEntryDisabledElseStopped, exit_code, "");
}

pub fn markFailedTarget(target: ApiIndexTarget, exit_code: i32, error_text: []const u8) i32 {
    return finishServiceTargetState(target, alwaysFailed, exit_code, error_text);
}

fn finishServiceTargetState(target: ApiIndexTarget, stateFor: *const fn (*const Entry) State, exit_code: i32, error_text: []const u8) i32 {
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        const slot = apiTargetSlotLocked(target, true) orelse {
            unlockRegistry(&registry_guard);
            return ERR_NOT_FOUND;
        };
        switch (tryLifecycleEndpointForSlot(slot)) {
            .contended => |ep| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(ep);
                continue;
            },
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                retireEndpointLocked(locked.endpoint, .cancelled);
                finishEntryState(&entries[slot], stateFor, exit_code, error_text);
                releaseTimedLock(&endpoint_guard);
            },
            .none => finishEntryState(&entries[slot], stateFor, exit_code, error_text),
        }
        unlockRegistry(&registry_guard);
        return OK;
    }
}

pub fn bumpRestartCount(name: []const u8) i32 {
    var registry_guard = lockRegistry(.registry_control);
    defer unlockRegistry(&registry_guard);
    const slot = findByNameIn(&entries, name) orelse return ERR_NOT_FOUND;
    entries[slot].restart_count +%= 1;
    return OK;
}

pub fn setStartMode(name: []const u8, start_mode: StartMode) i32 {
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        const slot = findByNameIn(&entries, name) orelse {
            unlockRegistry(&registry_guard);
            return ERR_NOT_FOUND;
        };
        if (start_mode != .disabled) {
            entries[slot].start_mode = start_mode;
            if (entries[slot].state == .disabled) entries[slot].state = .stopped;
            unlockRegistry(&registry_guard);
            return OK;
        }
        switch (tryLifecycleEndpointForSlot(slot)) {
            .contended => |ep| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(ep);
                continue;
            },
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                retireEndpointLocked(locked.endpoint, .cancelled);
                disableEntry(&entries[slot]);
                releaseTimedLock(&endpoint_guard);
            },
            .none => disableEntry(&entries[slot]),
        }
        unlockRegistry(&registry_guard);
        return OK;
    }
}

pub fn entryAt(index: usize) ?Entry {
    var registry_guard = lockRegistry(.registry_snapshot);
    defer unlockRegistry(&registry_guard);
    if (index >= entries.len or !entries[index].used) return null;
    return entries[index];
}

pub fn entryByName(name: []const u8) ?Entry {
    var registry_guard = lockRegistry(.registry_snapshot);
    defer unlockRegistry(&registry_guard);
    const slot = findByNameIn(&entries, name) orelse return null;
    return entries[slot];
}

pub fn countUsed() usize {
    var registry_guard = lockRegistry(.registry_snapshot);
    defer unlockRegistry(&registry_guard);
    return countUsedIn(&entries);
}

pub fn performanceSummary() PerformanceSummary {
    var out = PerformanceSummary{};
    var registry_guard = lockRegistry(.registry_snapshot);
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used) continue;
        out.used_services += 1;
        if (e.state == .running) out.running_services += 1;
    }
    out.registry_index_queries = registry_enumeration_performance.index_queries;
    out.registry_refresh_requests = registry_enumeration_performance.refresh_requests;
    out.registry_refresh_visits = registry_enumeration_performance.refresh_visits;
    out.registry_instance_lookups = registry_enumeration_performance.instance_lookups;
    out.registry_index_end_markers = registry_enumeration_performance.index_end_markers;
    i = 0;
    while (i < 3) : (i += 1) addLockTiming(&out, i, registry_lock_timing[i]);
    unlockRegistry(&registry_guard);

    i = 0;
    while (i < endpoints.len) : (i += 1) {
        var endpoint_guard = acquireTimedLock(
            &endpoints[i].lock,
            &endpoints[i].lock_timing[lockFamilyIndex(.endpoint_snapshot)],
        );
        if (!endpoint_guard.admitted) continue;
        const ep = &endpoints[i];
        addEndpointLifetime(&out, ep);
        var family_index: usize = 3;
        while (family_index < SERVICE_LOCK_FAMILY_COUNT) : (family_index += 1) {
            addLockTiming(&out, family_index, ep.lock_timing[family_index]);
        }
        if (!ep.used) {
            releaseTimedLock(&endpoint_guard);
            continue;
        }
        const counts = queueCounts(ep);
        out.endpoints_used += 1;
        out.request_pending +%= counts.queued + counts.delivered;
        out.response_pending +%= counts.responded;
        out.queue_depth_total +%= @intCast(API_ENDPOINT_QUEUE_DEPTH);
        out.queue_used_total +%= counts.used;
        out.queue_high_water_total +%= ep.queue_high_water;
        out.active_workers +%= counts.delivered;
        if (ep.max_active_workers > out.max_active_workers) out.max_active_workers = ep.max_active_workers;
        out.open_handles +%= ep.open_handles;
        out.requests +%= ep.requests;
        out.responses +%= ep.responses;
        out.drops +%= ep.drops;
        out.busy_rejections +%= ep.busy_rejections;
        out.timeouts +%= ep.timeouts;
        out.cancellations +%= ep.cancellations;
        out.completion_waits +%= ep.completion_waits;
        out.completion_timeouts +%= ep.completion_timeouts;
        out.completion_wait_rounds +%= ep.completion_wait_rounds;
        out.targeted_response_wakes +%= ep.targeted_response_wakes;
        out.targeted_response_wake_misses +%= ep.targeted_response_wake_misses;
        out.admission_waits +%= ep.admission_waits;
        out.admission_timeouts +%= ep.admission_timeouts;
        releaseTimedLock(&endpoint_guard);
    }
    return out;
}

pub fn apiInfoAt(index: u32, out: *ApiInfo, now_ticks: u64) i32 {
    var registry_guard = lockRegistry(.registry_snapshot);
    out.* = .{};
    const slot = apiSlotAtLocked(index) orelse {
        unlockRegistry(&registry_guard);
        return 0;
    };
    fillApiInfoEntryLocked(slot, out, now_ticks);
    const identity = endpointIdentityForSlotLocked(slot);
    unlockRegistry(&registry_guard);
    fillApiInfoEndpoint(identity, out);
    return 1;
}

pub fn apiDetailAt(index: u32, out: *ApiDetail, now_ticks: u64) i32 {
    var registry_guard = lockRegistry(.registry_snapshot);
    out.* = .{};
    const slot = apiSlotAtLocked(index) orelse {
        unlockRegistry(&registry_guard);
        return 0;
    };
    fillApiDetailEntryLocked(slot, out, now_ticks);
    const identity = endpointIdentityForSlotLocked(slot);
    unlockRegistry(&registry_guard);
    fillApiInfoEndpoint(identity, &out.info);
    return 1;
}

pub fn apiInfoForIndexTarget(target: ApiIndexTarget, out: *ApiInfo, now_ticks: u64) i32 {
    var registry_guard = lockRegistry(.registry_snapshot);
    out.* = .{};
    const slot = apiTargetSlotLocked(target, true) orelse {
        unlockRegistry(&registry_guard);
        return API_ERR_BUSY;
    };
    fillApiInfoEntryLocked(slot, out, now_ticks);
    const identity = endpointIdentityForSlotLocked(slot);
    unlockRegistry(&registry_guard);
    fillApiInfoEndpoint(identity, out);
    return 1;
}

pub fn apiDetailForIndexTarget(target: ApiIndexTarget, out: *ApiDetail, now_ticks: u64) i32 {
    var registry_guard = lockRegistry(.registry_snapshot);
    out.* = .{};
    const slot = apiTargetSlotLocked(target, true) orelse {
        unlockRegistry(&registry_guard);
        return API_ERR_BUSY;
    };
    fillApiDetailEntryLocked(slot, out, now_ticks);
    const identity = endpointIdentityForSlotLocked(slot);
    unlockRegistry(&registry_guard);
    fillApiInfoEndpoint(identity, &out.info);
    return 1;
}

pub fn apiStatus(name: []const u8, out: *ApiInfo, now_ticks: u64) i32 {
    var registry_guard = lockRegistry(.registry_snapshot);
    out.* = .{};
    const slot = findByNameIn(&entries, name) orelse {
        unlockRegistry(&registry_guard);
        return API_ERR_NOT_FOUND;
    };
    fillApiInfoEntryLocked(slot, out, now_ticks);
    const identity = endpointIdentityForSlotLocked(slot);
    unlockRegistry(&registry_guard);
    fillApiInfoEndpoint(identity, out);
    return API_OK;
}

pub fn apiDetailByName(name: []const u8, out: *ApiDetail, now_ticks: u64) i32 {
    var registry_guard = lockRegistry(.registry_snapshot);
    out.* = .{};
    const slot = findByNameIn(&entries, name) orelse {
        unlockRegistry(&registry_guard);
        return API_ERR_NOT_FOUND;
    };
    fillApiDetailEntryLocked(slot, out, now_ticks);
    const identity = endpointIdentityForSlotLocked(slot);
    unlockRegistry(&registry_guard);
    fillApiInfoEndpoint(identity, &out.info);
    return API_OK;
}

pub fn apiOpen(name: []const u8, out: *ApiInfo, now_ticks: u64) i32 {
    var registry_guard = lockRegistry(.registry_lookup);
    out.* = .{};
    const slot = findByNameIn(&entries, name) orelse {
        unlockRegistry(&registry_guard);
        return API_ERR_NOT_FOUND;
    };
    fillApiInfoEntryLocked(slot, out, now_ticks);
    if (entries[slot].state != .running or entries[slot].instance_id == 0) {
        unlockRegistry(&registry_guard);
        return API_ERR_NOT_RUNNING;
    }
    const identity = endpointIdentityForSlotLocked(slot) orelse {
        unlockRegistry(&registry_guard);
        return API_ERR_NO_ENDPOINT;
    };
    unlockRegistry(&registry_guard);

    var endpoint_lease = lockEndpointIdentity(identity, .endpoint_data) orelse return API_ERR_NOT_RUNNING;
    defer unlockEndpoint(&endpoint_lease);
    endpoint_lease.endpoint.open_handles +%= 1;
    fillApiInfoEndpointLocked(endpoint_lease.endpoint, out);
    return API_OK;
}

pub fn apiClose(handle: u32) i32 {
    var endpoint_lease = lockEndpointForHandle(handle, .endpoint_data) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    if (endpoint_lease.endpoint.open_handles > 0) endpoint_lease.endpoint.open_handles -= 1;
    return API_OK;
}

pub fn registerEndpoint(name: []const u8, instance_id: u32, flags: u32, out: *ApiInfo, now_ticks: u64) i32 {
    if (instance_id == 0) return API_ERR_INVALID;
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        out.* = .{};
        const slot = findByNameIn(&entries, name) orelse {
            unlockRegistry(&registry_guard);
            return API_ERR_NOT_FOUND;
        };
        fillApiInfoEntryLocked(slot, out, now_ticks);
        const e = &entries[slot];
        if (e.state != .running or e.instance_id != instance_id) {
            unlockRegistry(&registry_guard);
            return API_ERR_NOT_RUNNING;
        }

        const endpoint_index = endpointForSlot(slot) orelse freeEndpointSlot() orelse {
            unlockRegistry(&registry_guard);
            return API_ERR_FULL;
        };
        const ep = &endpoints[endpoint_index];
        switch (tryLifecycleEndpoint(ep)) {
            .contended => |contended| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(contended);
                continue;
            },
            .none => unreachable,
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                if (!ep.used) {
                    resetEndpointMetadata(ep, true);
                    advanceEndpointGeneration(ep);
                    ep.used = true;
                    ep.service_slot = slot;
                    ep.handle = allocateEndpointHandle();
                }
                ep.flags = flags;
                fillApiInfoEndpointLocked(ep, out);
                releaseTimedLock(&endpoint_guard);
                unlockRegistry(&registry_guard);
                return API_OK;
            },
        }
    }
}

pub fn unregisterEndpoint(handle: u32) i32 {
    while (true) {
        var registry_guard = lockRegistry(.registry_control);
        switch (tryLifecycleEndpointForHandle(handle)) {
            .none => {
                unlockRegistry(&registry_guard);
                return API_ERR_BAD_HANDLE;
            },
            .contended => |ep| {
                unlockRegistry(&registry_guard);
                waitForEndpointLifecycle(ep);
                continue;
            },
            .locked => |locked| {
                var endpoint_guard = locked.guard;
                retireEndpointLocked(locked.endpoint, .cancelled);
                releaseTimedLock(&endpoint_guard);
                unlockRegistry(&registry_guard);
                return API_OK;
            },
        }
    }
}

pub fn endpointPoll(handle: u32) i32 {
    var endpoint_lease = lockEndpointForHandle(handle, .endpoint_data) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    return @intCast(queueCounts(endpoint_lease.endpoint).queued);
}

// 0.56.19: Blockierendes Endpoint-API (Befund 8.5). Wartet auf der
// vorhandenen requests_available-WaitQueue des Endpoints, bis Requests
// anliegen oder der Timeout ablaeuft. Seit 0.69.9 uebergibt der persistente
// Endpoint-Lock atomar an die WaitQueue; weder Endpoint- noch Registry-Lock
// werden ueber den Schlaf gehalten. Das Praedikat schliesst weiterhin das
// Lost-Wakeup-Fenster zwischen Unlock und Enrollment.
const EndpointWaitContext = struct {
    identity: EndpointIdentity,
};

fn endpointWaitStillNeeded(raw: *anyopaque) bool {
    const ctx: *EndpointWaitContext = @ptrCast(@alignCast(raw));
    return identityMatches(ctx.identity.endpoint, ctx.identity) and
        !hasQueuedRequest(ctx.identity.endpoint);
}

pub fn endpointWait(handle: u32, timeout_ticks: u64) i32 {
    const identity = lookupEndpointIdentity(handle) orelse return API_ERR_BAD_HANDLE;
    var endpoint_lease = lockEndpointIdentity(identity, .endpoint_wait) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    const queued = queueCounts(endpoint_lease.endpoint).queued;
    if (queued > 0) {
        return @intCast(queued);
    }
    var wait_ctx = EndpointWaitContext{ .identity = identity };
    const wait_result = endpoint_lease.endpoint.requests_available.waitUnlessReleasing(
        timeout_ticks,
        "svc-endpoint",
        endpointWaitStillNeeded,
        &wait_ctx,
        releaseTimedLockForWait,
        &endpoint_lease.guard,
    );

    var result_lease = lockEndpointIdentity(identity, .endpoint_wait) orelse return switch (wait_result) {
        .cancelled, .killed => API_ERR_NOT_RUNNING,
        else => API_ERR_BAD_HANDLE,
    };
    defer unlockEndpoint(&result_lease);
    return @intCast(queueCounts(result_lease.endpoint).queued);
}

pub fn submitRequest(handle: u32, client_id: u32, op: u16, payload: []const u8) i32 {
    if (op == 0) return API_ERR_INVALID;
    if (payload.len > API_MAX_PAYLOAD) return API_ERR_PAYLOAD_TOO_LARGE;
    var endpoint_lease = lockEndpointForHandle(handle, .endpoint_data) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    const ep = endpoint_lease.endpoint;
    if (!ep.slots_available.tryAcquire()) {
        ep.busy_rejections +%= 1;
        return API_ERR_BUSY;
    }
    return publishRequestLocked(ep, client_id, op, payload);
}

pub fn submitRequestWait(handle: u32, client_id: u32, op: u16, payload: []const u8, timeout_ticks: u64) i32 {
    return submitRequestWaitInternal(handle, client_id, op, payload, timeout_ticks, null);
}

pub fn submitRequestWaitGuarded(
    handle: u32,
    client_id: u32,
    op: u16,
    payload: []const u8,
    timeout_ticks: u64,
    publish_unwind: *task_context.UnwindToken,
) i32 {
    publish_unwind.* = .{};
    return submitRequestWaitInternal(handle, client_id, op, payload, timeout_ticks, publish_unwind);
}

fn submitRequestWaitInternal(
    handle: u32,
    client_id: u32,
    op: u16,
    payload: []const u8,
    timeout_ticks: u64,
    publish_unwind: ?*task_context.UnwindToken,
) i32 {
    if (op == 0) return API_ERR_INVALID;
    if (payload.len > API_MAX_PAYLOAD) return API_ERR_PAYLOAD_TOO_LARGE;

    const forever = timeout_ticks == sync.WAIT_FOREVER;
    const deadline = if (forever) @as(u64, 0) else timer.deadlineAfterNow(timeout_ticks);
    const identity = lookupEndpointIdentity(handle) orelse return API_ERR_BAD_HANDLE;
    var endpoint_lease = lockEndpointIdentity(identity, .endpoint_wait) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    var ep = endpoint_lease.endpoint;
    if (timeout_ticks != 0 and !forever and timer.tickCount() >= deadline) {
        ep.timeouts +%= 1;
        ep.admission_timeouts +%= 1;
        return API_ERR_TIMEOUT;
    }
    if (ep.slots_available.tryAcquire()) {
        return publishRequestWithOptionalUnwindLocked(ep, client_id, op, payload, publish_unwind, .{});
    }

    if (timeout_ticks == 0) {
        ep.busy_rejections +%= 1;
        return API_ERR_BUSY;
    }

    const now = timer.tickCount();
    if (!forever and now >= deadline) {
        ep.timeouts +%= 1;
        ep.admission_timeouts +%= 1;
        return API_ERR_TIMEOUT;
    }
    ep.admission_waits +%= 1;
    const remaining = if (forever) sync.WAIT_FOREVER else deadline - now;
    var admission_unwind: task_context.UnwindToken = .{};
    const wait_result = ep.slots_available.acquireReleasingGuarded(
        remaining,
        releaseTimedLockForWait,
        &endpoint_lease.guard,
        &admission_unwind,
    );

    var result_lease = lockEndpointIdentity(identity, .endpoint_wait) orelse {
        leaveAdmissionUnwind(&admission_unwind);
        return switch (wait_result) {
            .cancelled, .killed => API_ERR_NOT_RUNNING,
            else => API_ERR_BAD_HANDLE,
        };
    };
    defer unlockEndpoint(&result_lease);
    ep = result_lease.endpoint;
    switch (wait_result) {
        .signaled => {},
        .timeout => {
            ep.timeouts +%= 1;
            ep.admission_timeouts +%= 1;
            return API_ERR_TIMEOUT;
        },
        .cancelled, .killed => {
            leaveAdmissionUnwind(&admission_unwind);
            return API_ERR_NOT_RUNNING;
        },
        .none, .failed => {
            leaveAdmissionUnwind(&admission_unwind);
            return API_ERR_BUSY;
        },
    }
    if (!forever and timer.tickCount() >= deadline) {
        _ = ep.slots_available.release(1);
        leaveAdmissionUnwind(&admission_unwind);
        ep.timeouts +%= 1;
        ep.admission_timeouts +%= 1;
        return API_ERR_TIMEOUT;
    }
    return publishRequestWithOptionalUnwindLocked(ep, client_id, op, payload, publish_unwind, admission_unwind);
}

fn publishRequestWithOptionalUnwindLocked(
    ep: *Endpoint,
    client_id: u32,
    op: u16,
    payload: []const u8,
    publish_unwind: ?*task_context.UnwindToken,
    acquired_unwind: task_context.UnwindToken,
) i32 {
    var owned_unwind = acquired_unwind;
    if (publish_unwind) |out| {
        if (!owned_unwind.active) {
            owned_unwind = task_context.enterUnwind();
            if (!owned_unwind.admitted()) {
                _ = ep.slots_available.release(1);
                return API_ERR_BUSY;
            }
        }
        out.* = owned_unwind;
    }
    const result = publishRequestLocked(ep, client_id, op, payload);
    if (result <= 0) {
        leaveAdmissionUnwind(&owned_unwind);
        if (publish_unwind) |out| out.* = .{};
    } else if (publish_unwind == null) {
        leaveAdmissionUnwind(&owned_unwind);
    }
    return result;
}

fn leaveAdmissionUnwind(unwind: *task_context.UnwindToken) void {
    if (unwind.active) _ = task_context.leaveUnwind(unwind.*);
    unwind.* = .{};
}

fn publishRequestLocked(ep: *Endpoint, client_id: u32, op: u16, payload: []const u8) i32 {
    const slot_idx = freeRequestSlot(ep) orelse {
        _ = ep.slots_available.release(1);
        ep.busy_rejections +%= 1;
        return API_ERR_BUSY;
    };
    var slot = &ep.queue[slot_idx];
    const request_id = allocateRequestId(ep);
    resetRequestSlotMetadata(slot);
    slot.request_id = request_id;
    slot.client_id = client_id;
    slot.op = op;
    slot.flags = ep.flags;
    slot.request_len = @intCast(payload.len);
    if (payload.len > 0) {
        @memcpy(slot.request_payload[0..payload.len], payload);
        ep.lifetime.payload_copy_bytes +%= payload.len;
    }
    // Der Zustand wird zuletzt publiziert. Damit sind nur die neu gesetzten
    // Laengen sichtbar; alte Bytes hinter request_len bleiben unerreichbar.
    slot.state = .queued;
    ep.requests +%= 1;
    noteQueueHighWater(ep);
    _ = ep.requests_available.wakeOne();
    return @intCast(request_id);
}

pub fn recvRequest(handle: u32, header: *ApiMessageHeader, out: []u8) i32 {
    clearMessageHeader(header);
    var endpoint_lease = lockEndpointForHandle(handle, .endpoint_data) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    const ep = endpoint_lease.endpoint;
    const slot_idx = queuedRequestSlot(ep) orelse return 0;
    var slot = &ep.queue[slot_idx];
    const len: usize = @intCast(slot.request_len);
    if (out.len < len) {
        fillMessageHeader(slot, header, slot.request_len, API_OK);
        return API_ERR_BUFFER_TOO_SMALL;
    }
    if (len > 0) {
        @memcpy(out[0..len], slot.request_payload[0..len]);
        ep.lifetime.payload_copy_bytes +%= len;
    }
    fillMessageHeader(slot, header, slot.request_len, API_OK);
    slot.state = .delivered;
    noteActiveWorkers(ep);
    return @intCast(len);
}

pub fn reply(handle: u32, request_id: u32, status: i32, payload: []const u8) i32 {
    if (payload.len > API_MAX_PAYLOAD) return API_ERR_PAYLOAD_TOO_LARGE;
    var endpoint_lease = lockEndpointForHandle(handle, .endpoint_data) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    const ep = endpoint_lease.endpoint;
    const slot_idx = deliveredRequestSlotById(ep, request_id) orelse return API_ERR_NOT_FOUND;
    var slot = &ep.queue[slot_idx];
    if (payload.len > 0) {
        @memcpy(slot.response_payload[0..payload.len], payload);
        ep.lifetime.payload_copy_bytes +%= payload.len;
    }
    slot.response_len = @intCast(payload.len);
    slot.response_status = status;
    slot.state = .responded;
    ep.responses +%= 1;
    if (slot.response_available.wakeOne() != 0) {
        ep.targeted_response_wakes +%= 1;
    } else {
        ep.targeted_response_wake_misses +%= 1;
    }
    return API_OK;
}

pub fn takeResponse(handle: u32, request_id: u32, header: *ApiMessageHeader, out: []u8) i32 {
    clearMessageHeader(header);
    var endpoint_lease = lockEndpointForHandle(handle, .endpoint_data) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    const ep = endpoint_lease.endpoint;
    const slot_idx = respondedRequestSlotById(ep, request_id) orelse return 0;
    var slot = &ep.queue[slot_idx];
    const len: usize = @intCast(slot.response_len);
    fillMessageHeader(slot, header, slot.response_len, slot.response_status);
    if (out.len < len) {
        clearRequestSlot(ep, slot);
        _ = ep.slots_available.release(1);
        return API_ERR_BUFFER_TOO_SMALL;
    }
    if (len > 0) {
        @memcpy(out[0..len], slot.response_payload[0..len]);
        ep.lifetime.payload_copy_bytes +%= len;
    }
    clearRequestSlot(ep, slot);
    _ = ep.slots_available.release(1);
    return @intCast(len);
}

const ResponseWaitContext = struct {
    identity: EndpointIdentity,
    slot: *RequestSlot,
    request_id: u32,
};

fn responseWaitStillNeeded(raw: *anyopaque) bool {
    const ctx: *ResponseWaitContext = @ptrCast(@alignCast(raw));
    return identityMatches(ctx.identity.endpoint, ctx.identity) and
        ctx.slot.request_id == ctx.request_id and
        ctx.slot.state != .free and
        ctx.slot.state != .responded;
}

pub fn waitResponse(handle: u32, request_id: u32, timeout_ticks: u64) i32 {
    if (request_id == 0) return API_ERR_INVALID;
    const identity = lookupEndpointIdentity(handle) orelse return API_ERR_BAD_HANDLE;
    var endpoint_lease = lockEndpointIdentity(identity, .endpoint_wait) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    var ep = endpoint_lease.endpoint;
    const slot_idx = requestSlotById(ep, request_id) orelse return API_ERR_NOT_FOUND;
    if (ep.queue[slot_idx].state == .responded) return API_OK;
    ep.completion_waits +%= 1;
    ep.completion_wait_rounds +%= 1;
    var wait_ctx = ResponseWaitContext{
        .identity = identity,
        .slot = &ep.queue[slot_idx],
        .request_id = request_id,
    };
    const wait_result = wait_ctx.slot.response_available.waitUnlessReleasing(
        timeout_ticks,
        "service-response",
        responseWaitStillNeeded,
        &wait_ctx,
        releaseTimedLockForWait,
        &endpoint_lease.guard,
    );

    var result_lease = lockEndpointIdentity(identity, .endpoint_wait) orelse return switch (wait_result) {
        .cancelled, .killed => API_ERR_NOT_RUNNING,
        else => API_ERR_BAD_HANDLE,
    };
    defer unlockEndpoint(&result_lease);
    ep = result_lease.endpoint;
    if (wait_result == .timeout) {
        ep.timeouts +%= 1;
        ep.completion_timeouts +%= 1;
        return API_ERR_TIMEOUT;
    }
    if (wait_result == .cancelled or wait_result == .killed) return API_ERR_NOT_RUNNING;
    if (wait_result == .none or wait_result == .failed) return API_ERR_BUSY;
    const result_slot = requestSlotById(ep, request_id) orelse return API_ERR_NOT_FOUND;
    return if (ep.queue[result_slot].state == .responded) API_OK else API_ERR_BUSY;
}

pub fn cancelRequest(handle: u32, request_id: u32) i32 {
    var endpoint_lease = lockEndpointForHandle(handle, .endpoint_data) orelse return API_ERR_BAD_HANDLE;
    defer unlockEndpoint(&endpoint_lease);
    const ep = endpoint_lease.endpoint;
    const slot_idx = requestSlotById(ep, request_id) orelse return API_ERR_NOT_FOUND;
    ep.drops +%= 1;
    ep.cancellations +%= 1;
    clearRequestSlot(ep, &ep.queue[slot_idx]);
    _ = ep.slots_available.release(1);
    return API_OK;
}

pub fn stateName(state: State) []const u8 {
    return switch (state) {
        .empty => "empty",
        .stopped => "stopped",
        .starting => "starting",
        .running => "running",
        .stopping => "stopping",
        .failed => "failed",
        .disabled => "disabled",
    };
}

pub fn startModeName(start_mode: StartMode) []const u8 {
    return switch (start_mode) {
        .manual => "manual",
        .auto => "auto",
        .disabled => "disabled",
    };
}

pub fn parseStartMode(value: []const u8) ?StartMode {
    if (nameEq(value, "manual")) return .manual;
    if (nameEq(value, "auto")) return .auto;
    if (nameEq(value, "disabled") or nameEq(value, "disable")) return .disabled;
    return null;
}

fn registerIn(table: *[MAX_SERVICES]Entry, name: []const u8, path: []const u8, args: []const u8, start_mode: StartMode, description: []const u8) i32 {
    if (!isValidName(name) or !hasR4xExtension(path) or path.len >= MAX_PATH or args.len >= MAX_ARGS or description.len >= MAX_DESCRIPTION) return ERR_INVALID;
    if (!isRegistryValueSafe(path) or !isRegistryValueSafe(args) or !isRegistryValueSafe(description)) return ERR_INVALID;
    if (findByNameIn(table, name) != null) return ERR_DUPLICATE;
    const slot = freeSlotIn(table) orelse return ERR_FULL;
    var e = &table[slot];
    e.* = .{
        .used = true,
        .state = if (start_mode == .disabled) .disabled else .stopped,
        .start_mode = start_mode,
    };
    e.name_len = copy(name, e.name[0..]);
    e.path_len = copy(path, e.path[0..]);
    e.args_len = copy(args, e.args[0..]);
    e.description_len = copy(description, e.description[0..]);
    return @intCast(slot);
}

fn unregisterIn(table: *[MAX_SERVICES]Entry, name: []const u8) i32 {
    const slot = findByNameIn(table, name) orelse return ERR_NOT_FOUND;
    table[slot] = .{};
    return OK;
}

fn setStateIn(table: *[MAX_SERVICES]Entry, name: []const u8, state: State, instance_id: u32, exit_code: i32, error_text: []const u8) i32 {
    if (state == .empty) return ERR_INVALID;
    const slot = findByNameIn(table, name) orelse return ERR_NOT_FOUND;
    var e = &table[slot];
    e.state = state;
    e.instance_id = instance_id;
    e.exit_code = exit_code;
    e.last_error_len = copy(error_text, e.last_error[0..]);
    return OK;
}

fn markRunningIn(table: *[MAX_SERVICES]Entry, name: []const u8, instance_id: u32, start_tick: u64) i32 {
    const slot = findByNameIn(table, name) orelse return ERR_NOT_FOUND;
    var e = &table[slot];
    if (e.start_mode == .disabled or instance_id == 0) return ERR_INVALID;
    e.state = .running;
    e.instance_id = instance_id;
    e.exit_code = 0;
    e.start_tick = start_tick;
    e.last_error_len = 0;
    return OK;
}

fn bumpRestartCountIn(table: *[MAX_SERVICES]Entry, name: []const u8) i32 {
    const slot = findByNameIn(table, name) orelse return ERR_NOT_FOUND;
    table[slot].restart_count +%= 1;
    return OK;
}

fn countUsedIn(table: *const [MAX_SERVICES]Entry) usize {
    var count: usize = 0;
    for (table) |entry| {
        if (entry.used) count += 1;
    }
    return count;
}

fn freeSlotIn(table: *const [MAX_SERVICES]Entry) ?usize {
    var i: usize = 0;
    while (i < table.len) : (i += 1) {
        if (!table[i].used) return i;
    }
    return null;
}

fn findByNameIn(table: *const [MAX_SERVICES]Entry, name: []const u8) ?usize {
    if (name.len == 0) return null;
    var i: usize = 0;
    while (i < table.len) : (i += 1) {
        const e = &table[i];
        if (e.used and nameEq(e.name[0..e.name_len], name)) return i;
    }
    return null;
}

fn resetEndpoints() void {
    for (&endpoints) |*ep| {
        if (ep.used) wakeEndpointWaiters(ep, .cancelled);
        resetEndpointMetadata(ep, false);
        ep.lock = sync.Mutex.initClass("service-endpoint", sync.LockRank.service_endpoint, .sleepable);
        ep.generation = 0;
        ep.request_id_next = 0;
        ep.request_id_limit = 0;
        ep.lifetime = .{};
        ep.lock_timing = .{LockTiming{}} ** SERVICE_LOCK_FAMILY_COUNT;
    }
    next_endpoint_handle = 1;
    next_request_id = 1;
}

fn resetRegistryState() void {
    entries = .{Entry{}} ** MAX_SERVICES;
    registry_lock = sync.Mutex.initClass("service-registry", sync.LockRank.service_registry, .sleepable);
    registry_lock_timing = .{LockTiming{}} ** SERVICE_LOCK_FAMILY_COUNT;
    api_index_slots = .{API_INDEX_INVALID_SLOT} ** MAX_SERVICES;
    api_index_count = 0;
    registry_generation = 1;
    registry_enumeration_performance = .{};
    resetEndpoints();
}

fn freeEndpointSlot() ?usize {
    var i: usize = 0;
    while (i < endpoints.len) : (i += 1) {
        if (!endpoints[i].used) return i;
    }
    return null;
}

fn endpointForSlot(slot: usize) ?usize {
    var i: usize = 0;
    while (i < endpoints.len) : (i += 1) {
        if (endpoints[i].used and endpoints[i].service_slot == slot) return i;
    }
    return null;
}

fn endpointForHandle(handle: u32) ?usize {
    if (handle == 0) return null;
    var i: usize = 0;
    while (i < endpoints.len) : (i += 1) {
        if (endpoints[i].used and endpoints[i].handle == handle) return i;
    }
    return null;
}

fn noteQueueScan(ep: *Endpoint, slots: usize) void {
    ep.lifetime.queue_scan_passes +%= 1;
    ep.lifetime.queue_scan_slots +%= slots;
}

fn freeRequestSlot(ep: *Endpoint) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == .free) {
            noteQueueScan(ep, i + 1);
            return i;
        }
    }
    noteQueueScan(ep, ep.queue.len);
    return null;
}

fn queuedRequestSlot(ep: *Endpoint) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == .queued) {
            noteQueueScan(ep, i + 1);
            return i;
        }
    }
    noteQueueScan(ep, ep.queue.len);
    return null;
}

fn hasQueuedRequest(ep: *const Endpoint) bool {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == .queued) return true;
    }
    return false;
}

fn requestSlotById(ep: *Endpoint, request_id: u32) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state != .free and ep.queue[i].request_id == request_id) {
            noteQueueScan(ep, i + 1);
            return i;
        }
    }
    noteQueueScan(ep, ep.queue.len);
    return null;
}

fn deliveredRequestSlotById(ep: *Endpoint, request_id: u32) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == .delivered and ep.queue[i].request_id == request_id) {
            noteQueueScan(ep, i + 1);
            return i;
        }
    }
    noteQueueScan(ep, ep.queue.len);
    return null;
}

fn respondedRequestSlotById(ep: *Endpoint, request_id: u32) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == .responded and ep.queue[i].request_id == request_id) {
            noteQueueScan(ep, i + 1);
            return i;
        }
    }
    noteQueueScan(ep, ep.queue.len);
    return null;
}

fn queueCounts(ep: *Endpoint) QueueCounts {
    var counts = QueueCounts{};
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        switch (ep.queue[i].state) {
            .free => {},
            .queued => counts.queued += 1,
            .delivered => counts.delivered += 1,
            .responded => counts.responded += 1,
        }
    }
    counts.used = counts.queued + counts.delivered + counts.responded;
    noteQueueScan(ep, ep.queue.len);
    return counts;
}

fn noteQueueHighWater(ep: *Endpoint) void {
    const used = queueCounts(ep).used;
    if (used > ep.queue_high_water) ep.queue_high_water = used;
}

fn noteActiveWorkers(ep: *Endpoint) void {
    const active = queueCounts(ep).delivered;
    if (active > ep.max_active_workers) ep.max_active_workers = active;
}

fn clearRequestSlot(ep: *Endpoint, slot: *RequestSlot) void {
    _ = slot.response_available.close(.cancelled);
    resetRequestSlotMetadata(slot);
    ep.lifetime.slot_metadata_resets +%= 1;
}

fn resetRequestSlotMetadata(slot: *RequestSlot) void {
    slot.state = .free;
    slot.request_id = 0;
    slot.client_id = 0;
    slot.op = 0;
    slot.flags = 0;
    slot.request_len = 0;
    slot.response_len = 0;
    slot.response_status = API_OK;
    slot.response_available = sync.WaitQueue.init();
}

fn retireEndpointLocked(ep: *Endpoint, result: sync.WaitResult) void {
    wakeEndpointWaiters(ep, result);
    resetEndpointMetadata(ep, true);
}

fn resetEndpointMetadata(ep: *Endpoint, count_reset: bool) void {
    ep.used = false;
    ep.service_slot = 0;
    ep.handle = 0;
    ep.flags = 0;
    ep.queue_high_water = 0;
    ep.max_active_workers = 0;
    ep.open_handles = 0;
    ep.requests = 0;
    ep.responses = 0;
    ep.drops = 0;
    ep.busy_rejections = 0;
    ep.timeouts = 0;
    ep.cancellations = 0;
    ep.completion_waits = 0;
    ep.completion_timeouts = 0;
    ep.completion_wait_rounds = 0;
    ep.targeted_response_wakes = 0;
    ep.targeted_response_wake_misses = 0;
    ep.admission_waits = 0;
    ep.admission_timeouts = 0;
    ep.requests_available = sync.WaitQueue.init();
    ep.slots_available = sync.Semaphore.init(@intCast(API_ENDPOINT_QUEUE_DEPTH), @intCast(API_ENDPOINT_QUEUE_DEPTH));
    for (&ep.queue) |*slot| resetRequestSlotMetadata(slot);
    if (count_reset) ep.lifetime.endpoint_metadata_resets +%= 1;
}

fn wakeEndpointWaiters(ep: *Endpoint, result: sync.WaitResult) void {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state != .free) {
            _ = ep.queue[i].response_available.close(result);
        }
    }
    _ = ep.requests_available.close(result);
    _ = ep.slots_available.queue.close(result);
}

fn advanceEndpointGeneration(ep: *Endpoint) void {
    ep.generation +%= 1;
    if (ep.generation == 0) ep.generation = 1;
}

fn allocateEndpointHandle() u32 {
    const handle = next_endpoint_handle;
    next_endpoint_handle +%= 1;
    if (next_endpoint_handle == 0) next_endpoint_handle = 1;
    return if (handle == 0) 1 else handle;
}

fn allocateRequestId(ep: *Endpoint) u32 {
    return allocateRequestIdFrom(ep, &next_request_id);
}

fn allocateRequestIdFrom(ep: *Endpoint, counter: *u32) u32 {
    if (ep.request_id_next != 0 and ep.request_id_next <= ep.request_id_limit) {
        const id = ep.request_id_next;
        ep.request_id_next += 1;
        return id;
    }

    var observed = @atomicLoad(u32, counter, .monotonic);
    while (true) {
        const first: u32 = if (observed == 0 or observed > REQUEST_ID_MAX) 1 else observed;
        const available = REQUEST_ID_MAX - first + 1;
        const count = @min(REQUEST_ID_BLOCK_SIZE, available);
        const after = first + count;
        const next_global: u32 = if (after > REQUEST_ID_MAX) 1 else after;
        if (@cmpxchgWeak(u32, counter, observed, next_global, .monotonic, .monotonic)) |actual| {
            observed = actual;
        } else {
            ep.request_id_next = first + 1;
            ep.request_id_limit = first + count - 1;
            return first;
        }
    }
}

fn fillApiInfoEntryLocked(slot: usize, out: *ApiInfo, now_ticks: u64) void {
    out.* = .{};
    if (slot >= entries.len or !entries[slot].used) return;
    const e = &entries[slot];
    out.state = stateCode(e.state);
    out.start_mode = startModeCode(e.start_mode);
    out.instance_id = e.instance_id;
    out.exit_code = e.exit_code;
    out.restart_count = e.restart_count;
    out.start_tick = e.start_tick;
    if (e.state == .running and e.start_tick != 0 and now_ticks >= e.start_tick) {
        out.uptime_ticks = now_ticks - e.start_tick;
    }
    if (e.name_len > 0) @memcpy(out.name[0..e.name_len], e.name[0..e.name_len]);
    if (e.last_error_len > 0) @memcpy(out.last_error[0..e.last_error_len], e.last_error[0..e.last_error_len]);
}

fn fillApiInfoEndpoint(identity: ?EndpointIdentity, out: *ApiInfo) void {
    const exact = identity orelse return;
    var endpoint_lease = lockEndpointIdentity(exact, .endpoint_snapshot) orelse return;
    defer unlockEndpoint(&endpoint_lease);
    fillApiInfoEndpointLocked(endpoint_lease.endpoint, out);
}

fn fillApiInfoEndpointLocked(ep: *Endpoint, out: *ApiInfo) void {
    const counts = queueCounts(ep);
    out.handle = ep.handle;
    out.flags |= API_FLAG_ENDPOINT;
    out.flags |= API_FLAG_QUEUE_BACKED;
    if (counts.queued != 0 or counts.delivered != 0) out.flags |= API_FLAG_REQUEST_PENDING;
    if (counts.responded != 0) out.flags |= API_FLAG_RESPONSE_PENDING;
    out.requests = ep.requests;
    out.responses = ep.responses;
    out.drops = ep.drops;
    out.queue_depth = @intCast(API_ENDPOINT_QUEUE_DEPTH);
    out.queue_used = counts.used;
    out.queue_high_water = ep.queue_high_water;
    out.active_workers = counts.delivered;
    out.max_active_workers = ep.max_active_workers;
    out.open_handles = ep.open_handles;
    out.busy_rejections = ep.busy_rejections;
    out.timeouts = ep.timeouts;
    out.cancellations = ep.cancellations;
}

fn fillApiDetailEntryLocked(slot: usize, out: *ApiDetail, now_ticks: u64) void {
    out.* = .{};
    if (slot >= entries.len or !entries[slot].used) return;
    const e = &entries[slot];
    fillApiInfoEntryLocked(slot, &out.info, now_ticks);
    if (e.path_len > 0) @memcpy(out.path[0..e.path_len], e.path[0..e.path_len]);
    if (e.args_len > 0) @memcpy(out.args[0..e.args_len], e.args[0..e.args_len]);
    if (e.description_len > 0) @memcpy(out.description[0..e.description_len], e.description[0..e.description_len]);
}

fn fillMessageHeader(slot: *const RequestSlot, header: *ApiMessageHeader, payload_len: u16, status: i32) void {
    header.* = .{
        .magic = API_MAGIC,
        .version = API_VERSION,
        .op = slot.op,
        .request_id = slot.request_id,
        .client_id = slot.client_id,
        .flags = slot.flags,
        .payload_len = payload_len,
        .status = status,
    };
}

fn clearMessageHeader(header: *ApiMessageHeader) void {
    header.* = .{
        .magic = 0,
        .version = 0,
    };
}

pub fn stateCode(state: State) u32 {
    return switch (state) {
        .empty => API_STATE_EMPTY,
        .stopped => API_STATE_STOPPED,
        .starting => API_STATE_STARTING,
        .running => API_STATE_RUNNING,
        .stopping => API_STATE_STOPPING,
        .failed => API_STATE_FAILED,
        .disabled => API_STATE_DISABLED,
    };
}

pub fn startModeCode(start_mode: StartMode) u32 {
    return switch (start_mode) {
        .manual => API_START_MANUAL,
        .auto => API_START_AUTO,
        .disabled => API_START_DISABLED,
    };
}

fn isValidName(name: []const u8) bool {
    if (name.len == 0 or name.len >= MAX_NAME) return false;
    for (name) |ch| {
        if (!isNameChar(ch)) return false;
    }
    return true;
}

fn isNameChar(ch: u8) bool {
    return (ch >= 'A' and ch <= 'Z') or
        (ch >= 'a' and ch <= 'z') or
        (ch >= '0' and ch <= '9') or
        ch == '.' or ch == '_' or ch == '-';
}

fn isRegistryValueSafe(value: []const u8) bool {
    for (value) |ch| {
        if (ch == ';' or ch == '\r' or ch == '\n') return false;
    }
    return true;
}

fn hasR4xExtension(path: []const u8) bool {
    if (path.len < 5) return false;
    return endsWithIgnoreCase(path, ".R4X");
}

fn endsWithIgnoreCase(s: []const u8, suffix: []const u8) bool {
    if (s.len < suffix.len) return false;
    return nameEq(s[s.len - suffix.len ..], suffix);
}

fn copy(src: []const u8, dst: []u8) usize {
    @memset(dst, 0);
    const len = @min(src.len, dst.len - 1);
    if (len > 0) @memcpy(dst[0..len], src[0..len]);
    return len;
}

fn nameEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(b[i])) return false;
    }
    return true;
}

fn upper(ch: u8) u8 {
    if (ch >= 'a' and ch <= 'z') return ch - ('a' - 'A');
    return ch;
}

test "dense service API index covers empty partial full and hole reuse" {
    resetRegistryState();
    var info: ApiInfo = .{};
    try std.testing.expectEqual(@as(i32, 0), apiInfoAt(0, &info, 0));
    try std.testing.expect(beginApiIndexRefresh(0) == null);

    try std.testing.expectEqual(@as(i32, 0), register("A", "C:\\A.R4X", "", .manual));
    try std.testing.expectEqual(@as(i32, 1), register("B", "C:\\B.R4X", "", .manual));
    try std.testing.expectEqual(@as(i32, 2), register("C", "C:\\C.R4X", "", .manual));
    try std.testing.expectEqual(OK, unregister("B"));
    try std.testing.expectEqual(@as(i32, 1), register("D", "C:\\D.R4X", "", .manual));

    try std.testing.expectEqual(@as(i32, 1), apiInfoAt(0, &info, 0));
    try std.testing.expectEqualStrings("A", info.name[0..1]);
    try std.testing.expectEqual(@as(i32, 1), apiInfoAt(1, &info, 0));
    try std.testing.expectEqualStrings("D", info.name[0..1]);
    try std.testing.expectEqual(@as(i32, 1), apiInfoAt(2, &info, 0));
    try std.testing.expectEqualStrings("C", info.name[0..1]);
    try std.testing.expectEqual(@as(i32, 0), apiInfoAt(3, &info, 0));

    var detail: ApiDetail = .{};
    try std.testing.expectEqual(@as(i32, 1), apiDetailAt(1, &detail, 0));
    try std.testing.expectEqualStrings("D", detail.info.name[0..1]);
    try std.testing.expectEqualStrings("C:\\D.R4X", detail.path[0..8]);

    var slot: usize = 3;
    while (slot < MAX_SERVICES) : (slot += 1) {
        var name_buf: [8]u8 = undefined;
        const name = try std.fmt.bufPrint(name_buf[0..], "S{d}", .{slot});
        var path_buf: [24]u8 = undefined;
        const path = try std.fmt.bufPrint(path_buf[0..], "C:\\S{d}.R4X", .{slot});
        try std.testing.expectEqual(@as(i32, @intCast(slot)), register(name, path, "", .manual));
    }
    try std.testing.expectEqual(ERR_FULL, register("FULL", "C:\\FULL.R4X", "", .manual));
    var index: u32 = 0;
    while (index < MAX_SERVICES) : (index += 1) {
        try std.testing.expect(beginApiIndexRefresh(index) != null);
    }
    try std.testing.expect(beginApiIndexRefresh(MAX_SERVICES) == null);
}

test "service API index target is linear measured and generation safe" {
    resetRegistryState();
    try std.testing.expectEqual(@as(i32, 0), register("ONE", "C:\\ONE.R4X", "", .manual));
    try std.testing.expectEqual(@as(i32, 1), register("TWO", "C:\\TWO.R4X", "", .manual));
    try std.testing.expectEqual(OK, markRunning("TWO", 42, 7));

    const first = beginApiIndexRefresh(0) orelse return error.TestUnexpectedResult;
    var second = beginApiIndexRefresh(1) orelse return error.TestUnexpectedResult;
    try std.testing.expect(beginApiIndexRefresh(2) == null);
    try std.testing.expect(noteApiIndexInstanceLookup(second));
    var summary = performanceSummary();
    try std.testing.expectEqual(@as(u64, 3), summary.registry_index_queries);
    try std.testing.expectEqual(@as(u64, 2), summary.registry_refresh_requests);
    try std.testing.expectEqual(@as(u64, 2), summary.registry_refresh_visits);
    try std.testing.expectEqual(@as(u64, 1), summary.registry_instance_lookups);
    try std.testing.expectEqual(@as(u64, 1), summary.registry_index_end_markers);

    var detail: ApiDetail = .{};
    try std.testing.expectEqual(@as(i32, 1), apiDetailForIndexTarget(second, &detail, 8));
    try std.testing.expectEqual(API_STATE_RUNNING, detail.info.state);
    try std.testing.expectEqualStrings("C:\\TWO.R4X", detail.path[0..10]);
    try std.testing.expectEqual(OK, markStoppingTarget(second));
    second.state = .stopping;
    try std.testing.expectEqual(@as(i32, 1), apiDetailForIndexTarget(second, &detail, 8));
    try std.testing.expectEqual(API_STATE_STOPPING, detail.info.state);
    try std.testing.expectEqual(OK, markStoppedTarget(second, 0));
    second.instance_id = 0;
    second.state = .stopped;
    var info: ApiInfo = .{};
    try std.testing.expectEqual(@as(i32, 1), apiInfoForIndexTarget(second, &info, 8));
    try std.testing.expectEqual(API_STATE_STOPPED, info.state);

    try std.testing.expectEqual(@as(i32, 2), register("THREE", "C:\\THREE.R4X", "", .manual));
    try std.testing.expectEqual(API_ERR_BUSY, apiInfoForIndexTarget(first, &info, 9));
    const retried = retryApiIndexRefresh(0) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i32, 1), apiInfoForIndexTarget(retried, &info, 9));
    summary = performanceSummary();
    try std.testing.expectEqual(@as(u64, 2), summary.registry_refresh_requests);
    try std.testing.expectEqual(@as(u64, 3), summary.registry_refresh_visits);
}

test "request slot metadata reset preserves payload storage" {
    var slot = RequestSlot{};
    @memset(slot.request_payload[0..], 0xA5);
    @memset(slot.response_payload[0..], 0x5A);
    slot.state = .responded;
    slot.request_id = 42;
    slot.client_id = 7;
    slot.op = 3;
    slot.flags = 9;
    slot.request_len = 4096;
    slot.response_len = 1;
    slot.response_status = -4;

    resetRequestSlotMetadata(&slot);

    try std.testing.expectEqual(RequestState.free, slot.state);
    try std.testing.expectEqual(@as(u32, 0), slot.request_id);
    try std.testing.expectEqual(@as(u16, 0), slot.request_len);
    try std.testing.expectEqual(@as(u16, 0), slot.response_len);
    try std.testing.expectEqual(API_OK, slot.response_status);
    try std.testing.expectEqual(@as(u8, 0xA5), slot.request_payload[0]);
    try std.testing.expectEqual(@as(u8, 0xA5), slot.request_payload[API_MAX_PAYLOAD - 1]);
    try std.testing.expectEqual(@as(u8, 0x5A), slot.response_payload[0]);
    try std.testing.expectEqual(@as(u8, 0x5A), slot.response_payload[API_MAX_PAYLOAD - 1]);
}

test "endpoint metadata reset preserves every slot payload" {
    var ep = Endpoint{};
    ep.used = true;
    ep.handle = 99;
    ep.generation = 7;
    ep.request_id_next = 41;
    ep.request_id_limit = 64;
    ep.requests = 12;
    ep.lifetime.payload_copy_bytes = 1234;
    ep.lock_timing[lockFamilyIndex(.endpoint_data)].acquisitions = 9;
    for (&ep.queue, 0..) |*slot, index| {
        @memset(slot.request_payload[0..], @intCast(index + 1));
        @memset(slot.response_payload[0..], @intCast(index + 17));
        slot.state = .responded;
        slot.request_len = @intCast(index + 1);
        slot.response_len = @intCast(index + 2);
    }

    resetEndpointMetadata(&ep, false);

    try std.testing.expect(!ep.used);
    try std.testing.expectEqual(@as(u32, 0), ep.handle);
    try std.testing.expectEqual(@as(u64, 7), ep.generation);
    try std.testing.expectEqual(@as(u32, 41), ep.request_id_next);
    try std.testing.expectEqual(@as(u32, 64), ep.request_id_limit);
    try std.testing.expectEqual(@as(u64, 0), ep.requests);
    try std.testing.expectEqual(@as(u64, 1234), ep.lifetime.payload_copy_bytes);
    try std.testing.expectEqual(@as(u64, 9), ep.lock_timing[lockFamilyIndex(.endpoint_data)].acquisitions);
    for (&ep.queue, 0..) |*slot, index| {
        try std.testing.expectEqual(RequestState.free, slot.state);
        try std.testing.expectEqual(@as(u16, 0), slot.request_len);
        try std.testing.expectEqual(@as(u16, 0), slot.response_len);
        try std.testing.expectEqual(@as(u8, @intCast(index + 1)), slot.request_payload[0]);
        try std.testing.expectEqual(@as(u8, @intCast(index + 1)), slot.request_payload[API_MAX_PAYLOAD - 1]);
        try std.testing.expectEqual(@as(u8, @intCast(index + 17)), slot.response_payload[0]);
        try std.testing.expectEqual(@as(u8, @intCast(index + 17)), slot.response_payload[API_MAX_PAYLOAD - 1]);
    }
}

test "request id blocks stay globally unique and endpoint local" {
    var counter: u32 = 1;
    var first = Endpoint{};
    var second = Endpoint{};

    try std.testing.expectEqual(@as(u32, 1), allocateRequestIdFrom(&first, &counter));
    try std.testing.expectEqual(@as(u32, 257), allocateRequestIdFrom(&second, &counter));
    try std.testing.expectEqual(@as(u32, 2), allocateRequestIdFrom(&first, &counter));
    try std.testing.expectEqual(@as(u32, 258), allocateRequestIdFrom(&second, &counter));
    try std.testing.expectEqual(@as(u32, 513), counter);

    var wrap = Endpoint{};
    counter = REQUEST_ID_MAX;
    try std.testing.expectEqual(REQUEST_ID_MAX, allocateRequestIdFrom(&wrap, &counter));
    try std.testing.expectEqual(@as(u32, 1), allocateRequestIdFrom(&wrap, &counter));
}

test "queue snapshot counts all states in one measured pass" {
    var ep = Endpoint{};
    ep.queue[0].state = .queued;
    ep.queue[1].state = .delivered;
    ep.queue[2].state = .responded;
    ep.queue[3].state = .queued;

    const counts = queueCounts(&ep);

    try std.testing.expectEqual(@as(u32, 2), counts.queued);
    try std.testing.expectEqual(@as(u32, 1), counts.delivered);
    try std.testing.expectEqual(@as(u32, 1), counts.responded);
    try std.testing.expectEqual(@as(u32, 4), counts.used);
    try std.testing.expectEqual(@as(u64, 1), ep.lifetime.queue_scan_passes);
    try std.testing.expectEqual(@as(u64, API_ENDPOINT_QUEUE_DEPTH), ep.lifetime.queue_scan_slots);
}

test "endpoint generation rejects a stale identity after immediate reuse" {
    var ep = Endpoint{};
    advanceEndpointGeneration(&ep);
    ep.used = true;
    ep.handle = 41;
    ep.service_slot = 3;
    const stale = EndpointIdentity{
        .endpoint = &ep,
        .handle = ep.handle,
        .generation = ep.generation,
        .service_slot = ep.service_slot,
    };

    resetEndpointMetadata(&ep, false);
    advanceEndpointGeneration(&ep);
    ep.used = true;
    ep.handle = 42;
    ep.service_slot = 3;

    try std.testing.expect(!identityMatches(&ep, stale));
    try std.testing.expectEqual(@as(u64, 2), ep.generation);
}

test "two endpoint slots retain independent locks queues and telemetry" {
    var first = Endpoint{};
    var second = Endpoint{};
    first.queue[0].state = .queued;
    second.queue[0].state = .responded;
    first.lock_timing[lockFamilyIndex(.endpoint_data)].acquisitions = 5;
    second.lock_timing[lockFamilyIndex(.endpoint_data)].acquisitions = 11;

    const first_counts = queueCounts(&first);
    const second_counts = queueCounts(&second);

    try std.testing.expect(@intFromPtr(&first.lock) != @intFromPtr(&second.lock));
    try std.testing.expectEqual(@as(u32, 1), first_counts.queued);
    try std.testing.expectEqual(@as(u32, 0), first_counts.responded);
    try std.testing.expectEqual(@as(u32, 0), second_counts.queued);
    try std.testing.expectEqual(@as(u32, 1), second_counts.responded);
    try std.testing.expectEqual(@as(u64, 5), first.lock_timing[lockFamilyIndex(.endpoint_data)].acquisitions);
    try std.testing.expectEqual(@as(u64, 11), second.lock_timing[lockFamilyIndex(.endpoint_data)].acquisitions);
}
