comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
// Native service registry and endpoint owner; this file transports fixed C
// records to unchanged Zig callers and retains the original owner fixtures.
const std = @import("std");
const config = @import("config");
const r4x_api = @import("../program/r4x_api.zig");
const sync = @import("../sched/sync.zig");
const task_context = @import("../sched/task_context.zig");

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

const RequestState = enum(u8) {
    free,
    queued,
    delivered,
    responded,
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

const LockTiming = extern struct {
    acquisitions: u64 = 0,
    contentions: u64 = 0,
    timing_samples: u64 = 0,
    wait_ns: u64 = 0,
    wait_max_ns: u64 = 0,
    hold_ns: u64 = 0,
    hold_max_ns: u64 = 0,
    timing_unavailable: u64 = 0,
};

const EndpointLifetimePerformance = extern struct {
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

pub const Entry = extern struct {
    used: bool = false,
    state: State = .empty,
    start_mode: StartMode = .manual,
    instance_id: u32 = 0,
    start_generation: u64 = 0,
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

pub const ApiIndexTarget = extern struct {
    registry_generation: u64 = 0,
    api_index: u32 = 0,
    service_slot: usize = 0,
    instance_id: u32 = 0,
    state: State = .empty,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
};

const RequestSlot = extern struct {
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

const Endpoint = extern struct {
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
    // Admission order is independent of payload-slot reuse and request-ID wrap.
    queued_slots: [API_ENDPOINT_QUEUE_DEPTH]usize = undefined,
    queued_count: usize = 0,
    queue: [API_ENDPOINT_QUEUE_DEPTH]RequestSlot = .{RequestSlot{}} ** API_ENDPOINT_QUEUE_DEPTH,
};

pub const PerformanceSummary = extern struct {
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
    lock_timing_stride: u32 = if (config.enable_metrics) @intCast(SERVICE_LOCK_TIMING_STRIDE) else 0,
    lock_timing_reserved0: u32 = 0,
    lock_timing_samples: [SERVICE_LOCK_FAMILY_COUNT]u64 = .{0} ** SERVICE_LOCK_FAMILY_COUNT,
    registry_index_queries: u64 = 0,
    registry_refresh_requests: u64 = 0,
    registry_refresh_visits: u64 = 0,
    registry_instance_lookups: u64 = 0,
    registry_index_end_markers: u64 = 0,
};

const RegistryEnumerationPerformance = extern struct {
    index_queries: u64 = 0,
    refresh_requests: u64 = 0,
    refresh_visits: u64 = 0,
    instance_lookups: u64 = 0,
    index_end_markers: u64 = 0,
};

pub const StartReservation = extern struct {
    slot: usize = 0,
    generation: u64 = 0,
    entry: Entry = .{},
};

const QueueCounts = extern struct {
    queued: u32 = 0,
    delivered: u32 = 0,
    responded: u32 = 0,
    used: u32 = 0,
};

const EndpointIdentity = extern struct {
    endpoint: *Endpoint,
    handle: u32,
    generation: u64,
    service_slot: usize,
};

const NativeBytes = extern struct { data: [*]const u8, len: usize };
fn nativeBytes(value: []const u8) NativeBytes {
    return .{ .data = value.ptr, .len = value.len };
}
fn emptyBytes() NativeBytes {
    return nativeBytes("");
}
extern fn r4os_rust_services_init() callconv(.c) void;
extern fn r4os_rust_services_register(NativeBytes, NativeBytes, NativeBytes, u8, NativeBytes) callconv(.c) i32;
extern fn r4os_rust_services_name(u8, NativeBytes, u8, u32, i32, u64, NativeBytes) callconv(.c) i32;
extern fn r4os_rust_services_target(u8, *const ApiIndexTarget, i32, NativeBytes) callconv(.c) i32;
extern fn r4os_rust_services_start(u8, NativeBytes, *StartReservation, u32, u64, i32, NativeBytes) callconv(.c) i32;
extern fn r4os_rust_services_get(u8, usize, NativeBytes, ?*const ApiIndexTarget, ?*Entry, ?*ApiIndexTarget) callconv(.c) bool;
extern fn r4os_rust_services_count() callconv(.c) usize;
extern fn r4os_rust_services_info(u8, u32, NativeBytes, ?*const ApiIndexTarget, *ApiInfo, u64) callconv(.c) i32;
extern fn r4os_rust_services_detail(u8, u32, NativeBytes, ?*const ApiIndexTarget, *ApiDetail, u64) callconv(.c) i32;
extern fn r4os_rust_services_endpoint_register(NativeBytes, u32, u32, *ApiInfo, u64) callconv(.c) i32;
extern fn r4os_rust_services_endpoint_control(u8, u32, u32, u64) callconv(.c) i32;
extern fn r4os_rust_services_request(u32, u32, u16, NativeBytes, u64, bool, ?*task_context.UnwindToken) callconv(.c) i32;
extern fn r4os_rust_services_recv(u32, u32, bool, *ApiMessageHeader, [*]u8, usize) callconv(.c) i32;
extern fn r4os_rust_services_reply(u32, u32, i32, NativeBytes) callconv(.c) i32;
extern fn r4os_rust_services_performance(*PerformanceSummary) callconv(.c) void;
extern fn r4os_rust_services_record(u8, ?*Endpoint, ?*RequestSlot, ?*u32, ?*QueueCounts, ?*const EndpointIdentity) callconv(.c) u32;
extern fn r4os_rust_services_label(bool, u8) callconv(.c) NativeBytes;
extern fn r4os_rust_services_parse(NativeBytes) callconv(.c) u8;

pub fn beginApiIndexRefresh(index: u32) ?ApiIndexTarget {
    var out: ApiIndexTarget = undefined;
    return if (r4os_rust_services_get(3, index, emptyBytes(), null, null, &out)) out else null;
}

pub fn retryApiIndexRefresh(index: u32) ?ApiIndexTarget {
    var out: ApiIndexTarget = undefined;
    return if (r4os_rust_services_get(4, index, emptyBytes(), null, null, &out)) out else null;
}

pub fn entryForApiIndexTarget(target: ApiIndexTarget) ?Entry {
    var out: Entry = undefined;
    return if (r4os_rust_services_get(2, 0, emptyBytes(), &target, &out, null)) out else null;
}

pub fn noteApiIndexInstanceLookup(target: ApiIndexTarget) bool {
    return r4os_rust_services_target(3, &target, 0, emptyBytes()) != 0;
}

pub fn init() void {
    r4os_rust_services_init();
}

pub fn register(name: []const u8, path: []const u8, args: []const u8, start_mode: StartMode) i32 {
    return registerWithDescription(name, path, args, start_mode, "");
}

pub fn registerWithDescription(name: []const u8, path: []const u8, args: []const u8, start_mode: StartMode, description: []const u8) i32 {
    return r4os_rust_services_register(nativeBytes(name), nativeBytes(path), nativeBytes(args), @intFromEnum(start_mode), nativeBytes(description));
}

pub fn unregister(name: []const u8) i32 {
    return r4os_rust_services_name(0, nativeBytes(name), 0, 0, 0, 0, emptyBytes());
}

pub fn setState(name: []const u8, state: State, instance_id: u32, exit_code: i32, error_text: []const u8) i32 {
    return r4os_rust_services_name(1, nativeBytes(name), @intFromEnum(state), instance_id, exit_code, 0, nativeBytes(error_text));
}

pub fn beginStart(name: []const u8, out: *StartReservation) i32 {
    return r4os_rust_services_start(0, nativeBytes(name), out, 0, 0, 0, emptyBytes());
}

pub fn markStarting(name: []const u8) i32 {
    return r4os_rust_services_name(2, nativeBytes(name), 0, 0, 0, 0, emptyBytes());
}

pub fn completeStart(reservation: *const StartReservation, instance_id: u32, start_tick: u64) bool {
    return r4os_rust_services_start(1, emptyBytes(), @constCast(reservation), instance_id, start_tick, 0, emptyBytes()) != 0;
}

pub fn failStart(reservation: *const StartReservation, exit_code: i32, error_text: []const u8) void {
    _ = r4os_rust_services_start(2, emptyBytes(), @constCast(reservation), 0, 0, exit_code, nativeBytes(error_text));
}

pub fn cancelStarting(name: []const u8, generation: u64) void {
    _ = r4os_rust_services_name(9, nativeBytes(name), 0, 0, 0, generation, emptyBytes());
}

pub fn markRunning(name: []const u8, instance_id: u32, start_tick: u64) i32 {
    return r4os_rust_services_name(3, nativeBytes(name), 0, instance_id, 0, start_tick, emptyBytes());
}

pub fn markStopping(name: []const u8) i32 {
    return r4os_rust_services_name(4, nativeBytes(name), 0, 0, 0, 0, emptyBytes());
}

pub fn markStopped(name: []const u8, exit_code: i32) i32 {
    return r4os_rust_services_name(5, nativeBytes(name), 0, 0, exit_code, 0, emptyBytes());
}

pub fn markFailed(name: []const u8, exit_code: i32, error_text: []const u8) i32 {
    return r4os_rust_services_name(6, nativeBytes(name), 0, 0, exit_code, 0, nativeBytes(error_text));
}

pub fn markStoppingTarget(target: ApiIndexTarget) i32 {
    return r4os_rust_services_target(0, &target, 0, emptyBytes());
}

pub fn markStoppedTarget(target: ApiIndexTarget, exit_code: i32) i32 {
    return r4os_rust_services_target(1, &target, exit_code, emptyBytes());
}

pub fn markFailedTarget(target: ApiIndexTarget, exit_code: i32, error_text: []const u8) i32 {
    return r4os_rust_services_target(2, &target, exit_code, nativeBytes(error_text));
}

pub fn bumpRestartCount(name: []const u8) i32 {
    return r4os_rust_services_name(7, nativeBytes(name), 0, 0, 0, 0, emptyBytes());
}

pub fn setStartMode(name: []const u8, start_mode: StartMode) i32 {
    return r4os_rust_services_name(8, nativeBytes(name), @intFromEnum(start_mode), 0, 0, 0, emptyBytes());
}

pub fn entryAt(index: usize) ?Entry {
    var out: Entry = undefined;
    return if (r4os_rust_services_get(0, index, emptyBytes(), null, &out, null)) out else null;
}

pub fn entryByName(name: []const u8) ?Entry {
    var out: Entry = undefined;
    return if (r4os_rust_services_get(1, 0, nativeBytes(name), null, &out, null)) out else null;
}

pub fn countUsed() usize {
    return r4os_rust_services_count();
}

pub fn performanceSummary() PerformanceSummary {
    var out: PerformanceSummary = undefined;
    r4os_rust_services_performance(&out);
    return out;
}

pub fn apiInfoAt(index: u32, out: *ApiInfo, now_ticks: u64) i32 {
    return r4os_rust_services_info(0, index, emptyBytes(), null, out, now_ticks);
}

pub fn apiDetailAt(index: u32, out: *ApiDetail, now_ticks: u64) i32 {
    return r4os_rust_services_detail(0, index, emptyBytes(), null, out, now_ticks);
}

pub fn apiInfoForIndexTarget(target: ApiIndexTarget, out: *ApiInfo, now_ticks: u64) i32 {
    return r4os_rust_services_info(1, 0, emptyBytes(), &target, out, now_ticks);
}

pub fn apiDetailForIndexTarget(target: ApiIndexTarget, out: *ApiDetail, now_ticks: u64) i32 {
    return r4os_rust_services_detail(1, 0, emptyBytes(), &target, out, now_ticks);
}

pub fn apiStatus(name: []const u8, out: *ApiInfo, now_ticks: u64) i32 {
    return r4os_rust_services_info(2, 0, nativeBytes(name), null, out, now_ticks);
}

pub fn apiDetailByName(name: []const u8, out: *ApiDetail, now_ticks: u64) i32 {
    return r4os_rust_services_detail(2, 0, nativeBytes(name), null, out, now_ticks);
}

pub fn apiOpen(name: []const u8, out: *ApiInfo, now_ticks: u64) i32 {
    return r4os_rust_services_info(3, 0, nativeBytes(name), null, out, now_ticks);
}

pub fn apiClose(handle: u32) i32 {
    return r4os_rust_services_endpoint_control(0, handle, 0, 0);
}

pub fn registerEndpoint(name: []const u8, instance_id: u32, flags: u32, out: *ApiInfo, now_ticks: u64) i32 {
    return r4os_rust_services_endpoint_register(nativeBytes(name), instance_id, flags, out, now_ticks);
}

pub fn unregisterEndpoint(handle: u32) i32 {
    return r4os_rust_services_endpoint_control(1, handle, 0, 0);
}

pub fn endpointPoll(handle: u32) i32 {
    return r4os_rust_services_endpoint_control(2, handle, 0, 0);
}

pub fn endpointWait(handle: u32, timeout_ticks: u64) i32 {
    return r4os_rust_services_endpoint_control(3, handle, 0, timeout_ticks);
}

pub fn submitRequest(handle: u32, client_id: u32, op: u16, payload: []const u8) i32 {
    return r4os_rust_services_request(handle, client_id, op, nativeBytes(payload), 0, false, null);
}

pub fn submitRequestWait(handle: u32, client_id: u32, op: u16, payload: []const u8, timeout_ticks: u64) i32 {
    return r4os_rust_services_request(handle, client_id, op, nativeBytes(payload), timeout_ticks, true, null);
}

pub fn submitRequestWaitGuarded(
    handle: u32,
    client_id: u32,
    op: u16,
    payload: []const u8,
    timeout_ticks: u64,
    publish_unwind: *task_context.UnwindToken,
) i32 {
    return r4os_rust_services_request(handle, client_id, op, nativeBytes(payload), timeout_ticks, true, publish_unwind);
}

pub fn recvRequest(handle: u32, header: *ApiMessageHeader, out: []u8) i32 {
    return r4os_rust_services_recv(handle, 0, true, header, out.ptr, out.len);
}

pub fn reply(handle: u32, request_id: u32, status: i32, payload: []const u8) i32 {
    return r4os_rust_services_reply(handle, request_id, status, nativeBytes(payload));
}

pub fn takeResponse(handle: u32, request_id: u32, header: *ApiMessageHeader, out: []u8) i32 {
    return r4os_rust_services_recv(handle, request_id, false, header, out.ptr, out.len);
}

pub fn waitResponse(handle: u32, request_id: u32, timeout_ticks: u64) i32 {
    return r4os_rust_services_endpoint_control(4, handle, request_id, timeout_ticks);
}

pub fn cancelRequest(handle: u32, request_id: u32) i32 {
    return r4os_rust_services_endpoint_control(5, handle, request_id, 0);
}

pub fn stateName(state: State) []const u8 {
    const value = r4os_rust_services_label(false, @intFromEnum(state));
    return value.data[0..value.len];
}

pub fn startModeName(start_mode: StartMode) []const u8 {
    const value = r4os_rust_services_label(true, @intFromEnum(start_mode));
    return value.data[0..value.len];
}

pub fn parseStartMode(value: []const u8) ?StartMode {
    const result = r4os_rust_services_parse(nativeBytes(value));
    return if (result == 255) null else @enumFromInt(result);
}

pub fn stateCode(state: State) u32 {
    return @intFromEnum(state);
}

pub fn startModeCode(start_mode: StartMode) u32 {
    return @as(u32, @intFromEnum(start_mode)) + 1;
}

// Existing metadata cases call these same productive native algorithms.
fn resetRegistryState() void {
    r4os_rust_services_init();
}
fn resetRequestSlotMetadata(slot: *RequestSlot) void {
    _ = r4os_rust_services_record(0, null, slot, null, null, null);
}
fn resetEndpointMetadata(ep: *Endpoint, count_reset: bool) void {
    _ = r4os_rust_services_record(if (count_reset) 2 else 1, ep, null, null, null, null);
}
fn allocateRequestIdFrom(ep: *Endpoint, counter: *u32) u32 {
    return r4os_rust_services_record(3, ep, null, counter, null, null);
}
fn queueCounts(ep: *Endpoint) QueueCounts {
    var out: QueueCounts = undefined;
    _ = r4os_rust_services_record(4, ep, null, null, &out, null);
    return out;
}
fn advanceEndpointGeneration(ep: *Endpoint) void {
    _ = r4os_rust_services_record(5, ep, null, null, null, null);
}
fn identityMatches(ep: *const Endpoint, identity: EndpointIdentity) bool {
    return r4os_rust_services_record(6, @constCast(ep), null, null, null, &identity) != 0;
}
fn lockFamilyIndex(family: LockFamily) usize {
    return @intFromEnum(family);
}

comptime {
    if (@offsetOf(LockTiming, "acquisitions") != 0) @compileError("Native service LockTiming.acquisitions offset drift");

    if (@offsetOf(LockTiming, "contentions") != 8) @compileError("Native service LockTiming.contentions offset drift");

    if (@offsetOf(LockTiming, "timing_samples") != 16) @compileError("Native service LockTiming.timing_samples offset drift");

    if (@offsetOf(LockTiming, "wait_ns") != 24) @compileError("Native service LockTiming.wait_ns offset drift");

    if (@offsetOf(LockTiming, "wait_max_ns") != 32) @compileError("Native service LockTiming.wait_max_ns offset drift");

    if (@offsetOf(LockTiming, "hold_ns") != 40) @compileError("Native service LockTiming.hold_ns offset drift");

    if (@offsetOf(LockTiming, "hold_max_ns") != 48) @compileError("Native service LockTiming.hold_max_ns offset drift");

    if (@offsetOf(LockTiming, "timing_unavailable") != 56) @compileError("Native service LockTiming.timing_unavailable offset drift");

    if (@sizeOf(LockTiming) != 64 or @alignOf(LockTiming) != 8) @compileError("Native service LockTiming layout drift");

    if (@offsetOf(EndpointLifetimePerformance, "payload_copy_bytes") != 0) @compileError("Native service EndpointLifetimePerformance.payload_copy_bytes offset drift");

    if (@offsetOf(EndpointLifetimePerformance, "payload_clear_bytes") != 8) @compileError("Native service EndpointLifetimePerformance.payload_clear_bytes offset drift");

    if (@offsetOf(EndpointLifetimePerformance, "slot_metadata_resets") != 16) @compileError("Native service EndpointLifetimePerformance.slot_metadata_resets offset drift");

    if (@offsetOf(EndpointLifetimePerformance, "endpoint_metadata_resets") != 24) @compileError("Native service EndpointLifetimePerformance.endpoint_metadata_resets offset drift");

    if (@offsetOf(EndpointLifetimePerformance, "endpoint_payload_reset_bytes") != 32) @compileError("Native service EndpointLifetimePerformance.endpoint_payload_reset_bytes offset drift");

    if (@offsetOf(EndpointLifetimePerformance, "queue_scan_passes") != 40) @compileError("Native service EndpointLifetimePerformance.queue_scan_passes offset drift");

    if (@offsetOf(EndpointLifetimePerformance, "queue_scan_slots") != 48) @compileError("Native service EndpointLifetimePerformance.queue_scan_slots offset drift");

    if (@offsetOf(EndpointLifetimePerformance, "revalidations") != 56) @compileError("Native service EndpointLifetimePerformance.revalidations offset drift");

    if (@offsetOf(EndpointLifetimePerformance, "stale_rejections") != 64) @compileError("Native service EndpointLifetimePerformance.stale_rejections offset drift");

    if (@sizeOf(EndpointLifetimePerformance) != 72 or @alignOf(EndpointLifetimePerformance) != 8) @compileError("Native service EndpointLifetimePerformance layout drift");

    if (@offsetOf(Entry, "used") != 0) @compileError("Native service Entry.used offset drift");

    if (@offsetOf(Entry, "state") != 1) @compileError("Native service Entry.state offset drift");

    if (@offsetOf(Entry, "start_mode") != 2) @compileError("Native service Entry.start_mode offset drift");

    if (@offsetOf(Entry, "instance_id") != 4) @compileError("Native service Entry.instance_id offset drift");

    if (@offsetOf(Entry, "start_generation") != 8) @compileError("Native service Entry.start_generation offset drift");

    if (@offsetOf(Entry, "exit_code") != 16) @compileError("Native service Entry.exit_code offset drift");

    if (@offsetOf(Entry, "start_tick") != 24) @compileError("Native service Entry.start_tick offset drift");

    if (@offsetOf(Entry, "restart_count") != 32) @compileError("Native service Entry.restart_count offset drift");

    if (@offsetOf(Entry, "name") != 36) @compileError("Native service Entry.name offset drift");

    if (@offsetOf(Entry, "name_len") != 72) @compileError("Native service Entry.name_len offset drift");

    if (@offsetOf(Entry, "path") != 80) @compileError("Native service Entry.path offset drift");

    if (@offsetOf(Entry, "path_len") != 1104) @compileError("Native service Entry.path_len offset drift");

    if (@offsetOf(Entry, "args") != 1112) @compileError("Native service Entry.args offset drift");

    if (@offsetOf(Entry, "args_len") != 1208) @compileError("Native service Entry.args_len offset drift");

    if (@offsetOf(Entry, "description") != 1216) @compileError("Native service Entry.description offset drift");

    if (@offsetOf(Entry, "description_len") != 1296) @compileError("Native service Entry.description_len offset drift");

    if (@offsetOf(Entry, "last_error") != 1304) @compileError("Native service Entry.last_error offset drift");

    if (@offsetOf(Entry, "last_error_len") != 1368) @compileError("Native service Entry.last_error_len offset drift");

    if (@sizeOf(Entry) != 1376 or @alignOf(Entry) != 8) @compileError("Native service Entry layout drift");

    if (@offsetOf(ApiIndexTarget, "registry_generation") != 0) @compileError("Native service ApiIndexTarget.registry_generation offset drift");

    if (@offsetOf(ApiIndexTarget, "api_index") != 8) @compileError("Native service ApiIndexTarget.api_index offset drift");

    if (@offsetOf(ApiIndexTarget, "service_slot") != 16) @compileError("Native service ApiIndexTarget.service_slot offset drift");

    if (@offsetOf(ApiIndexTarget, "instance_id") != 24) @compileError("Native service ApiIndexTarget.instance_id offset drift");

    if (@offsetOf(ApiIndexTarget, "state") != 28) @compileError("Native service ApiIndexTarget.state offset drift");

    if (@offsetOf(ApiIndexTarget, "name") != 29) @compileError("Native service ApiIndexTarget.name offset drift");

    if (@offsetOf(ApiIndexTarget, "name_len") != 64) @compileError("Native service ApiIndexTarget.name_len offset drift");

    if (@sizeOf(ApiIndexTarget) != 72 or @alignOf(ApiIndexTarget) != 8) @compileError("Native service ApiIndexTarget layout drift");

    if (@offsetOf(RequestSlot, "state") != 0) @compileError("Native service RequestSlot.state offset drift");

    if (@offsetOf(RequestSlot, "request_id") != 4) @compileError("Native service RequestSlot.request_id offset drift");

    if (@offsetOf(RequestSlot, "client_id") != 8) @compileError("Native service RequestSlot.client_id offset drift");

    if (@offsetOf(RequestSlot, "op") != 12) @compileError("Native service RequestSlot.op offset drift");

    if (@offsetOf(RequestSlot, "flags") != 16) @compileError("Native service RequestSlot.flags offset drift");

    if (@offsetOf(RequestSlot, "request_len") != 20) @compileError("Native service RequestSlot.request_len offset drift");

    if (@offsetOf(RequestSlot, "response_len") != 22) @compileError("Native service RequestSlot.response_len offset drift");

    if (@offsetOf(RequestSlot, "response_status") != 24) @compileError("Native service RequestSlot.response_status offset drift");

    if (@offsetOf(RequestSlot, "response_available") != 32) @compileError("Native service RequestSlot.response_available offset drift");

    if (@offsetOf(RequestSlot, "request_payload") != 72) @compileError("Native service RequestSlot.request_payload offset drift");

    if (@offsetOf(RequestSlot, "response_payload") != 4168) @compileError("Native service RequestSlot.response_payload offset drift");

    if (@sizeOf(RequestSlot) != 8264 or @alignOf(RequestSlot) != 8) @compileError("Native service RequestSlot layout drift");

    if (@offsetOf(Endpoint, "lock") != 0) @compileError("Native service Endpoint.lock offset drift");

    if (@offsetOf(Endpoint, "generation") != 88) @compileError("Native service Endpoint.generation offset drift");

    if (@offsetOf(Endpoint, "request_id_next") != 96) @compileError("Native service Endpoint.request_id_next offset drift");

    if (@offsetOf(Endpoint, "request_id_limit") != 100) @compileError("Native service Endpoint.request_id_limit offset drift");

    if (@offsetOf(Endpoint, "lifetime") != 104) @compileError("Native service Endpoint.lifetime offset drift");

    if (@offsetOf(Endpoint, "lock_timing") != 176) @compileError("Native service Endpoint.lock_timing offset drift");

    if (@offsetOf(Endpoint, "used") != 624) @compileError("Native service Endpoint.used offset drift");

    if (@offsetOf(Endpoint, "service_slot") != 632) @compileError("Native service Endpoint.service_slot offset drift");

    if (@offsetOf(Endpoint, "handle") != 640) @compileError("Native service Endpoint.handle offset drift");

    if (@offsetOf(Endpoint, "flags") != 644) @compileError("Native service Endpoint.flags offset drift");

    if (@offsetOf(Endpoint, "queue_high_water") != 648) @compileError("Native service Endpoint.queue_high_water offset drift");

    if (@offsetOf(Endpoint, "max_active_workers") != 652) @compileError("Native service Endpoint.max_active_workers offset drift");

    if (@offsetOf(Endpoint, "open_handles") != 656) @compileError("Native service Endpoint.open_handles offset drift");

    if (@offsetOf(Endpoint, "requests") != 664) @compileError("Native service Endpoint.requests offset drift");

    if (@offsetOf(Endpoint, "responses") != 672) @compileError("Native service Endpoint.responses offset drift");

    if (@offsetOf(Endpoint, "drops") != 680) @compileError("Native service Endpoint.drops offset drift");

    if (@offsetOf(Endpoint, "busy_rejections") != 688) @compileError("Native service Endpoint.busy_rejections offset drift");

    if (@offsetOf(Endpoint, "timeouts") != 696) @compileError("Native service Endpoint.timeouts offset drift");

    if (@offsetOf(Endpoint, "cancellations") != 704) @compileError("Native service Endpoint.cancellations offset drift");

    if (@offsetOf(Endpoint, "completion_waits") != 712) @compileError("Native service Endpoint.completion_waits offset drift");

    if (@offsetOf(Endpoint, "completion_timeouts") != 720) @compileError("Native service Endpoint.completion_timeouts offset drift");

    if (@offsetOf(Endpoint, "completion_wait_rounds") != 728) @compileError("Native service Endpoint.completion_wait_rounds offset drift");

    if (@offsetOf(Endpoint, "targeted_response_wakes") != 736) @compileError("Native service Endpoint.targeted_response_wakes offset drift");

    if (@offsetOf(Endpoint, "targeted_response_wake_misses") != 744) @compileError("Native service Endpoint.targeted_response_wake_misses offset drift");

    if (@offsetOf(Endpoint, "admission_waits") != 752) @compileError("Native service Endpoint.admission_waits offset drift");

    if (@offsetOf(Endpoint, "admission_timeouts") != 760) @compileError("Native service Endpoint.admission_timeouts offset drift");

    if (@offsetOf(Endpoint, "requests_available") != 768) @compileError("Native service Endpoint.requests_available offset drift");

    if (@offsetOf(Endpoint, "slots_available") != 808) @compileError("Native service Endpoint.slots_available offset drift");

    if (@offsetOf(Endpoint, "queued_slots") != 856) @compileError("Native service Endpoint.queued_slots offset drift");

    if (@offsetOf(Endpoint, "queued_count") != 920) @compileError("Native service Endpoint.queued_count offset drift");

    if (@offsetOf(Endpoint, "queue") != 928) @compileError("Native service Endpoint.queue offset drift");

    if (@sizeOf(Endpoint) != 67040 or @alignOf(Endpoint) != 8) @compileError("Native service Endpoint layout drift");

    if (@offsetOf(PerformanceSummary, "max_services") != 0) @compileError("Native service PerformanceSummary.max_services offset drift");

    if (@offsetOf(PerformanceSummary, "used_services") != 4) @compileError("Native service PerformanceSummary.used_services offset drift");

    if (@offsetOf(PerformanceSummary, "running_services") != 8) @compileError("Native service PerformanceSummary.running_services offset drift");

    if (@offsetOf(PerformanceSummary, "endpoints_used") != 12) @compileError("Native service PerformanceSummary.endpoints_used offset drift");

    if (@offsetOf(PerformanceSummary, "request_pending") != 16) @compileError("Native service PerformanceSummary.request_pending offset drift");

    if (@offsetOf(PerformanceSummary, "response_pending") != 20) @compileError("Native service PerformanceSummary.response_pending offset drift");

    if (@offsetOf(PerformanceSummary, "queue_depth_total") != 24) @compileError("Native service PerformanceSummary.queue_depth_total offset drift");

    if (@offsetOf(PerformanceSummary, "queue_used_total") != 28) @compileError("Native service PerformanceSummary.queue_used_total offset drift");

    if (@offsetOf(PerformanceSummary, "queue_high_water_total") != 32) @compileError("Native service PerformanceSummary.queue_high_water_total offset drift");

    if (@offsetOf(PerformanceSummary, "active_workers") != 36) @compileError("Native service PerformanceSummary.active_workers offset drift");

    if (@offsetOf(PerformanceSummary, "max_active_workers") != 40) @compileError("Native service PerformanceSummary.max_active_workers offset drift");

    if (@offsetOf(PerformanceSummary, "open_handles") != 44) @compileError("Native service PerformanceSummary.open_handles offset drift");

    if (@offsetOf(PerformanceSummary, "requests") != 48) @compileError("Native service PerformanceSummary.requests offset drift");

    if (@offsetOf(PerformanceSummary, "responses") != 56) @compileError("Native service PerformanceSummary.responses offset drift");

    if (@offsetOf(PerformanceSummary, "drops") != 64) @compileError("Native service PerformanceSummary.drops offset drift");

    if (@offsetOf(PerformanceSummary, "busy_rejections") != 72) @compileError("Native service PerformanceSummary.busy_rejections offset drift");

    if (@offsetOf(PerformanceSummary, "timeouts") != 80) @compileError("Native service PerformanceSummary.timeouts offset drift");

    if (@offsetOf(PerformanceSummary, "cancellations") != 88) @compileError("Native service PerformanceSummary.cancellations offset drift");

    if (@offsetOf(PerformanceSummary, "completion_waits") != 96) @compileError("Native service PerformanceSummary.completion_waits offset drift");

    if (@offsetOf(PerformanceSummary, "completion_timeouts") != 104) @compileError("Native service PerformanceSummary.completion_timeouts offset drift");

    if (@offsetOf(PerformanceSummary, "completion_wait_rounds") != 112) @compileError("Native service PerformanceSummary.completion_wait_rounds offset drift");

    if (@offsetOf(PerformanceSummary, "targeted_response_wakes") != 120) @compileError("Native service PerformanceSummary.targeted_response_wakes offset drift");

    if (@offsetOf(PerformanceSummary, "targeted_response_wake_misses") != 128) @compileError("Native service PerformanceSummary.targeted_response_wake_misses offset drift");

    if (@offsetOf(PerformanceSummary, "admission_waits") != 136) @compileError("Native service PerformanceSummary.admission_waits offset drift");

    if (@offsetOf(PerformanceSummary, "admission_timeouts") != 144) @compileError("Native service PerformanceSummary.admission_timeouts offset drift");

    if (@offsetOf(PerformanceSummary, "payload_copy_bytes") != 152) @compileError("Native service PerformanceSummary.payload_copy_bytes offset drift");

    if (@offsetOf(PerformanceSummary, "payload_clear_bytes") != 160) @compileError("Native service PerformanceSummary.payload_clear_bytes offset drift");

    if (@offsetOf(PerformanceSummary, "slot_metadata_resets") != 168) @compileError("Native service PerformanceSummary.slot_metadata_resets offset drift");

    if (@offsetOf(PerformanceSummary, "endpoint_metadata_resets") != 176) @compileError("Native service PerformanceSummary.endpoint_metadata_resets offset drift");

    if (@offsetOf(PerformanceSummary, "endpoint_payload_reset_bytes") != 184) @compileError("Native service PerformanceSummary.endpoint_payload_reset_bytes offset drift");

    if (@offsetOf(PerformanceSummary, "queue_scan_passes") != 192) @compileError("Native service PerformanceSummary.queue_scan_passes offset drift");

    if (@offsetOf(PerformanceSummary, "queue_scan_slots") != 200) @compileError("Native service PerformanceSummary.queue_scan_slots offset drift");

    if (@offsetOf(PerformanceSummary, "endpoint_revalidations") != 208) @compileError("Native service PerformanceSummary.endpoint_revalidations offset drift");

    if (@offsetOf(PerformanceSummary, "endpoint_stale_rejections") != 216) @compileError("Native service PerformanceSummary.endpoint_stale_rejections offset drift");

    if (@offsetOf(PerformanceSummary, "lock_family_count") != 224) @compileError("Native service PerformanceSummary.lock_family_count offset drift");

    if (@offsetOf(PerformanceSummary, "lock_reserved0") != 228) @compileError("Native service PerformanceSummary.lock_reserved0 offset drift");

    if (@offsetOf(PerformanceSummary, "lock_acquisitions") != 232) @compileError("Native service PerformanceSummary.lock_acquisitions offset drift");

    if (@offsetOf(PerformanceSummary, "lock_contentions") != 288) @compileError("Native service PerformanceSummary.lock_contentions offset drift");

    if (@offsetOf(PerformanceSummary, "lock_wait_ns") != 344) @compileError("Native service PerformanceSummary.lock_wait_ns offset drift");

    if (@offsetOf(PerformanceSummary, "lock_wait_max_ns") != 400) @compileError("Native service PerformanceSummary.lock_wait_max_ns offset drift");

    if (@offsetOf(PerformanceSummary, "lock_hold_ns") != 456) @compileError("Native service PerformanceSummary.lock_hold_ns offset drift");

    if (@offsetOf(PerformanceSummary, "lock_hold_max_ns") != 512) @compileError("Native service PerformanceSummary.lock_hold_max_ns offset drift");

    if (@offsetOf(PerformanceSummary, "lock_timing_unavailable") != 568) @compileError("Native service PerformanceSummary.lock_timing_unavailable offset drift");

    if (@offsetOf(PerformanceSummary, "lock_timing_stride") != 624) @compileError("Native service PerformanceSummary.lock_timing_stride offset drift");

    if (@offsetOf(PerformanceSummary, "lock_timing_reserved0") != 628) @compileError("Native service PerformanceSummary.lock_timing_reserved0 offset drift");

    if (@offsetOf(PerformanceSummary, "lock_timing_samples") != 632) @compileError("Native service PerformanceSummary.lock_timing_samples offset drift");

    if (@offsetOf(PerformanceSummary, "registry_index_queries") != 688) @compileError("Native service PerformanceSummary.registry_index_queries offset drift");

    if (@offsetOf(PerformanceSummary, "registry_refresh_requests") != 696) @compileError("Native service PerformanceSummary.registry_refresh_requests offset drift");

    if (@offsetOf(PerformanceSummary, "registry_refresh_visits") != 704) @compileError("Native service PerformanceSummary.registry_refresh_visits offset drift");

    if (@offsetOf(PerformanceSummary, "registry_instance_lookups") != 712) @compileError("Native service PerformanceSummary.registry_instance_lookups offset drift");

    if (@offsetOf(PerformanceSummary, "registry_index_end_markers") != 720) @compileError("Native service PerformanceSummary.registry_index_end_markers offset drift");

    if (@sizeOf(PerformanceSummary) != 728 or @alignOf(PerformanceSummary) != 8) @compileError("Native service PerformanceSummary layout drift");

    if (@offsetOf(RegistryEnumerationPerformance, "index_queries") != 0) @compileError("Native service RegistryEnumerationPerformance.index_queries offset drift");

    if (@offsetOf(RegistryEnumerationPerformance, "refresh_requests") != 8) @compileError("Native service RegistryEnumerationPerformance.refresh_requests offset drift");

    if (@offsetOf(RegistryEnumerationPerformance, "refresh_visits") != 16) @compileError("Native service RegistryEnumerationPerformance.refresh_visits offset drift");

    if (@offsetOf(RegistryEnumerationPerformance, "instance_lookups") != 24) @compileError("Native service RegistryEnumerationPerformance.instance_lookups offset drift");

    if (@offsetOf(RegistryEnumerationPerformance, "index_end_markers") != 32) @compileError("Native service RegistryEnumerationPerformance.index_end_markers offset drift");

    if (@sizeOf(RegistryEnumerationPerformance) != 40 or @alignOf(RegistryEnumerationPerformance) != 8) @compileError("Native service RegistryEnumerationPerformance layout drift");

    if (@offsetOf(StartReservation, "slot") != 0) @compileError("Native service StartReservation.slot offset drift");

    if (@offsetOf(StartReservation, "generation") != 8) @compileError("Native service StartReservation.generation offset drift");

    if (@offsetOf(StartReservation, "entry") != 16) @compileError("Native service StartReservation.entry offset drift");

    if (@sizeOf(StartReservation) != 1392 or @alignOf(StartReservation) != 8) @compileError("Native service StartReservation layout drift");

    if (@offsetOf(QueueCounts, "queued") != 0) @compileError("Native service QueueCounts.queued offset drift");

    if (@offsetOf(QueueCounts, "delivered") != 4) @compileError("Native service QueueCounts.delivered offset drift");

    if (@offsetOf(QueueCounts, "responded") != 8) @compileError("Native service QueueCounts.responded offset drift");

    if (@offsetOf(QueueCounts, "used") != 12) @compileError("Native service QueueCounts.used offset drift");

    if (@sizeOf(QueueCounts) != 16 or @alignOf(QueueCounts) != 4) @compileError("Native service QueueCounts layout drift");

    if (@sizeOf(EndpointIdentity) != 32 or @offsetOf(EndpointIdentity, "endpoint") != 0 or
        @offsetOf(EndpointIdentity, "handle") != 8 or @offsetOf(EndpointIdentity, "generation") != 16 or
        @offsetOf(EndpointIdentity, "service_slot") != 24 or @sizeOf(NativeBytes) != 16)
        @compileError("Native service identity/bytes layout drift");
    if (@intFromEnum(State.empty) != API_STATE_EMPTY or @intFromEnum(State.disabled) != API_STATE_DISABLED or
        @intFromEnum(StartMode.manual) + 1 != API_START_MANUAL or @intFromEnum(StartMode.disabled) + 1 != API_START_DISABLED)
        @compileError("Native service state/mode conversion drift");
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
