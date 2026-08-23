const r4x_api = @import("../program/r4x_api.zig");
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

const Endpoint = struct {
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
};

var entries: [MAX_SERVICES]Entry = .{Entry{}} ** MAX_SERVICES;
var endpoints: [MAX_ENDPOINTS]Endpoint = .{Endpoint{}} ** MAX_ENDPOINTS;
var next_endpoint_handle: u32 = 1;
var next_request_id: u32 = 1;
var registry_lock = sync.Mutex.initClass("service-registry", sync.LockRank.service_registry, .sleepable);

fn lockRegistry() bool {
    return registry_lock.lock(sync.WAIT_FOREVER);
}

fn unlockRegistry(locked: bool) void {
    if (locked) _ = registry_lock.unlock();
}

pub fn init() void {
    resetRegistryState();
}

pub fn register(name: []const u8, path: []const u8, args: []const u8, start_mode: StartMode) i32 {
    return registerWithDescription(name, path, args, start_mode, "");
}

pub fn registerWithDescription(name: []const u8, path: []const u8, args: []const u8, start_mode: StartMode, description: []const u8) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    return registerIn(&entries, name, path, args, start_mode, description);
}

pub fn unregister(name: []const u8) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    if (findByNameIn(&entries, name)) |slot| clearEndpointForSlot(slot);
    return unregisterIn(&entries, name);
}

pub fn setState(name: []const u8, state: State, instance_id: u32, exit_code: i32, error_text: []const u8) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    if (state == .stopped or state == .failed or state == .disabled) {
        if (findByNameIn(&entries, name)) |slot| clearEndpointForSlot(slot);
    }
    return setStateIn(&entries, name, state, instance_id, exit_code, error_text);
}

pub fn markStarting(name: []const u8) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const slot = findByNameIn(&entries, name) orelse return ERR_NOT_FOUND;
    clearEndpointForSlot(slot);
    var e = &entries[slot];
    if (e.start_mode == .disabled) return ERR_INVALID;
    e.state = .starting;
    e.instance_id = 0;
    e.exit_code = 0;
    e.last_error_len = 0;
    return OK;
}

pub fn markRunning(name: []const u8, instance_id: u32, start_tick: u64) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
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
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const slot = findByNameIn(&entries, name) orelse return ERR_NOT_FOUND;
    var e = &entries[slot];
    if (e.state != .running and e.state != .starting) return ERR_INVALID;
    clearEndpointForSlot(slot);
    e.state = .stopping;
    return OK;
}

pub fn markStopped(name: []const u8, exit_code: i32) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const slot = findByNameIn(&entries, name) orelse return ERR_NOT_FOUND;
    clearEndpointForSlot(slot);
    var e = &entries[slot];
    e.state = if (e.start_mode == .disabled) .disabled else .stopped;
    e.instance_id = 0;
    e.exit_code = exit_code;
    e.start_tick = 0;
    e.last_error_len = 0;
    return OK;
}

pub fn markFailed(name: []const u8, exit_code: i32, error_text: []const u8) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const slot = findByNameIn(&entries, name) orelse return ERR_NOT_FOUND;
    clearEndpointForSlot(slot);
    var e = &entries[slot];
    e.state = .failed;
    e.instance_id = 0;
    e.exit_code = exit_code;
    e.start_tick = 0;
    e.last_error_len = copy(error_text, e.last_error[0..]);
    return OK;
}

pub fn bumpRestartCount(name: []const u8) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const slot = findByNameIn(&entries, name) orelse return ERR_NOT_FOUND;
    entries[slot].restart_count +%= 1;
    return OK;
}

pub fn setStartMode(name: []const u8, start_mode: StartMode) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const slot = findByNameIn(&entries, name) orelse return ERR_NOT_FOUND;
    entries[slot].start_mode = start_mode;
    if (start_mode == .disabled) {
        clearEndpointForSlot(slot);
        entries[slot].state = .disabled;
        entries[slot].instance_id = 0;
        entries[slot].start_tick = 0;
    } else if (entries[slot].state == .disabled) {
        entries[slot].state = .stopped;
    }
    return OK;
}

pub fn entryAt(index: usize) ?*const Entry {
    if (index >= entries.len or !entries[index].used) return null;
    return &entries[index];
}

pub fn entryByName(name: []const u8) ?*const Entry {
    const slot = findByNameIn(&entries, name) orelse return null;
    return &entries[slot];
}

pub fn countUsed() usize {
    return countUsedIn(&entries);
}

pub fn performanceSummary() PerformanceSummary {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    var out = PerformanceSummary{};
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used) continue;
        out.used_services += 1;
        if (e.state == .running) out.running_services += 1;
    }
    i = 0;
    while (i < endpoints.len) : (i += 1) {
        const ep = &endpoints[i];
        if (!ep.used) continue;
        const queued = countQueuedSlots(ep);
        const active = countDeliveredSlots(ep);
        const responded = countRespondedSlots(ep);
        out.endpoints_used += 1;
        out.request_pending +%= queued + active;
        out.response_pending +%= responded;
        out.queue_depth_total +%= @intCast(API_ENDPOINT_QUEUE_DEPTH);
        out.queue_used_total +%= countUsedSlots(ep);
        out.queue_high_water_total +%= ep.queue_high_water;
        out.active_workers +%= active;
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
    }
    return out;
}

pub fn apiInfoAt(index: u32, out: *ApiInfo, now_ticks: u64) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    out.* = .{};
    var seen: u32 = 0;
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        if (!entries[i].used) continue;
        if (seen == index) {
            fillApiInfo(i, out, now_ticks);
            return 1;
        }
        seen += 1;
    }
    return 0;
}

pub fn apiDetailAt(index: u32, out: *ApiDetail, now_ticks: u64) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    out.* = .{};
    var seen: u32 = 0;
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        if (!entries[i].used) continue;
        if (seen == index) {
            fillApiDetail(i, out, now_ticks);
            return 1;
        }
        seen += 1;
    }
    return 0;
}

pub fn apiStatus(name: []const u8, out: *ApiInfo, now_ticks: u64) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    out.* = .{};
    const slot = findByNameIn(&entries, name) orelse return API_ERR_NOT_FOUND;
    fillApiInfo(slot, out, now_ticks);
    return API_OK;
}

pub fn apiDetailByName(name: []const u8, out: *ApiDetail, now_ticks: u64) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    out.* = .{};
    const slot = findByNameIn(&entries, name) orelse return API_ERR_NOT_FOUND;
    fillApiDetail(slot, out, now_ticks);
    return API_OK;
}

pub fn apiOpen(name: []const u8, out: *ApiInfo, now_ticks: u64) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    out.* = .{};
    const slot = findByNameIn(&entries, name) orelse return API_ERR_NOT_FOUND;
    if (entries[slot].state != .running or entries[slot].instance_id == 0) {
        fillApiInfo(slot, out, now_ticks);
        return API_ERR_NOT_RUNNING;
    }
    if (endpointForSlot(slot) == null) {
        fillApiInfo(slot, out, now_ticks);
        return API_ERR_NO_ENDPOINT;
    }
    if (endpointForSlot(slot)) |idx| {
        endpoints[idx].open_handles +%= 1;
    }
    fillApiInfo(slot, out, now_ticks);
    return API_OK;
}

pub fn apiClose(handle: u32) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const idx = endpointForHandle(handle) orelse return API_ERR_BAD_HANDLE;
    var ep = &endpoints[idx];
    if (ep.open_handles > 0) ep.open_handles -= 1;
    return API_OK;
}

pub fn registerEndpoint(name: []const u8, instance_id: u32, flags: u32, out: *ApiInfo, now_ticks: u64) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    out.* = .{};
    if (instance_id == 0) return API_ERR_INVALID;
    const slot = findByNameIn(&entries, name) orelse return API_ERR_NOT_FOUND;
    const e = &entries[slot];
    if (e.state != .running or e.instance_id != instance_id) {
        fillApiInfo(slot, out, now_ticks);
        return API_ERR_NOT_RUNNING;
    }
    if (endpointForSlot(slot)) |idx| {
        endpoints[idx].flags = flags;
        fillApiInfo(slot, out, now_ticks);
        return API_OK;
    }
    const free = freeEndpointSlot() orelse return API_ERR_FULL;
    endpoints[free] = .{
        .used = true,
        .service_slot = slot,
        .handle = allocateEndpointHandle(),
        .flags = flags,
    };
    fillApiInfo(slot, out, now_ticks);
    return API_OK;
}

pub fn unregisterEndpoint(handle: u32) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const idx = endpointForHandle(handle) orelse return API_ERR_BAD_HANDLE;
    wakeEndpointWaiters(&endpoints[idx], .cancelled);
    endpoints[idx] = .{};
    return API_OK;
}

pub fn endpointPoll(handle: u32) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const idx = endpointForHandle(handle) orelse return API_ERR_BAD_HANDLE;
    const ep = &endpoints[idx];
    if (!serviceSlotRunning(ep.service_slot)) return API_ERR_NOT_RUNNING;
    return @intCast(countQueuedSlots(ep));
}

// 0.56.19: Blockierendes Endpoint-API (Befund 8.5). Wartet auf der
// vorhandenen requests_available-WaitQueue des Endpoints, bis Requests
// anliegen oder der Timeout ablaeuft. MEMSUITE-Regel: der Registry-Lock
// wird NIE ueber den Schlaf gehalten; das waitUnless-Praedikat schliesst
// das Lost-Wakeup-Fenster zwischen Unlock und addWaiter (submitRequest
// koennte dazwischen queuen und wakeOne ins Leere feuern).
const EndpointWaitContext = struct {
    endpoint: *Endpoint,
    handle: u32,
};

fn endpointWaitStillNeeded(raw: *anyopaque) bool {
    const ctx: *EndpointWaitContext = @ptrCast(@alignCast(raw));
    return ctx.endpoint.used and
        ctx.endpoint.handle == ctx.handle and
        countQueuedSlots(ctx.endpoint) == 0;
}

fn releaseMutexForWait(raw: *anyopaque) void {
    const mutex: *sync.Mutex = @ptrCast(@alignCast(raw));
    _ = mutex.unlock();
}

pub fn endpointWait(handle: u32, timeout_ticks: u64) i32 {
    const locked = lockRegistry();
    const idx = endpointForHandle(handle) orelse {
        unlockRegistry(locked);
        return API_ERR_BAD_HANDLE;
    };
    const ep = &endpoints[idx];
    if (!serviceSlotRunning(ep.service_slot)) {
        unlockRegistry(locked);
        return API_ERR_NOT_RUNNING;
    }
    const queued = countQueuedSlots(ep);
    if (queued > 0) {
        unlockRegistry(locked);
        return @intCast(queued);
    }
    var wait_ctx = EndpointWaitContext{ .endpoint = ep, .handle = handle };
    _ = ep.requests_available.waitUnlessReleasing(
        timeout_ticks,
        "svc-endpoint",
        endpointWaitStillNeeded,
        &wait_ctx,
        releaseMutexForWait,
        &registry_lock,
    );

    const result_locked = lockRegistry();
    defer unlockRegistry(result_locked);
    const result_idx = endpointForHandle(handle) orelse return API_ERR_BAD_HANDLE;
    const result_ep = &endpoints[result_idx];
    if (!serviceSlotRunning(result_ep.service_slot)) return API_ERR_NOT_RUNNING;
    return @intCast(countQueuedSlots(result_ep));
}

pub fn submitRequest(handle: u32, client_id: u32, op: u16, payload: []const u8) i32 {
    if (op == 0) return API_ERR_INVALID;
    if (payload.len > API_MAX_PAYLOAD) return API_ERR_PAYLOAD_TOO_LARGE;
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const idx = endpointForHandle(handle) orelse return API_ERR_BAD_HANDLE;
    var ep = &endpoints[idx];
    if (!serviceSlotRunning(ep.service_slot)) return API_ERR_NOT_RUNNING;
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
    const deadline = if (forever) @as(u64, 0) else timer.tickCount() +| timeout_ticks;
    const locked = lockRegistry();
    const idx = endpointForHandle(handle) orelse {
        unlockRegistry(locked);
        return API_ERR_BAD_HANDLE;
    };
    var ep = &endpoints[idx];
    if (!serviceSlotRunning(ep.service_slot)) {
        unlockRegistry(locked);
        return API_ERR_NOT_RUNNING;
    }
    if (timeout_ticks != 0 and !forever and timer.tickCount() >= deadline) {
        ep.timeouts +%= 1;
        ep.admission_timeouts +%= 1;
        unlockRegistry(locked);
        return API_ERR_TIMEOUT;
    }
    if (ep.slots_available.tryAcquire()) {
        const result = publishRequestWithOptionalUnwindLocked(ep, client_id, op, payload, publish_unwind, .{});
        unlockRegistry(locked);
        return result;
    }

    if (timeout_ticks == 0) {
        ep.busy_rejections +%= 1;
        unlockRegistry(locked);
        return API_ERR_BUSY;
    }

    const now = timer.tickCount();
    if (!forever and now >= deadline) {
        ep.timeouts +%= 1;
        ep.admission_timeouts +%= 1;
        unlockRegistry(locked);
        return API_ERR_TIMEOUT;
    }
    ep.admission_waits +%= 1;
    const remaining = if (forever) sync.WAIT_FOREVER else deadline - now;
    var admission_unwind: task_context.UnwindToken = .{};
    const wait_result = ep.slots_available.acquireReleasingGuarded(
        remaining,
        releaseMutexForWait,
        &registry_lock,
        &admission_unwind,
    );

    const result_locked = lockRegistry();
    defer unlockRegistry(result_locked);
    const result_idx = endpointForHandle(handle) orelse {
        leaveAdmissionUnwind(&admission_unwind);
        return switch (wait_result) {
            .cancelled, .killed => API_ERR_NOT_RUNNING,
            else => API_ERR_BAD_HANDLE,
        };
    };
    ep = &endpoints[result_idx];
    if (!serviceSlotRunning(ep.service_slot)) {
        leaveAdmissionUnwind(&admission_unwind);
        return API_ERR_NOT_RUNNING;
    }
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
    const request_id = allocateRequestId();
    slot.* = .{
        .state = .queued,
        .request_id = request_id,
        .client_id = client_id,
        .op = op,
        .flags = ep.flags,
        .request_len = @intCast(payload.len),
        .response_status = API_OK,
    };
    if (payload.len > 0) @memcpy(slot.request_payload[0..payload.len], payload);
    ep.requests +%= 1;
    noteQueueHighWater(ep);
    _ = ep.requests_available.wakeOne();
    return @intCast(request_id);
}

pub fn recvRequest(handle: u32, header: *ApiMessageHeader, out: []u8) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    clearMessageHeader(header);
    const idx = endpointForHandle(handle) orelse return API_ERR_BAD_HANDLE;
    var ep = &endpoints[idx];
    if (!serviceSlotRunning(ep.service_slot)) return API_ERR_NOT_RUNNING;
    const slot_idx = queuedRequestSlot(ep) orelse return 0;
    var slot = &ep.queue[slot_idx];
    const len: usize = @intCast(slot.request_len);
    if (out.len < len) {
        fillMessageHeader(slot, header, slot.request_len, API_OK);
        return API_ERR_BUFFER_TOO_SMALL;
    }
    if (len > 0) @memcpy(out[0..len], slot.request_payload[0..len]);
    fillMessageHeader(slot, header, slot.request_len, API_OK);
    slot.state = .delivered;
    noteActiveWorkers(ep);
    return @intCast(len);
}

pub fn reply(handle: u32, request_id: u32, status: i32, payload: []const u8) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    if (payload.len > API_MAX_PAYLOAD) return API_ERR_PAYLOAD_TOO_LARGE;
    const idx = endpointForHandle(handle) orelse return API_ERR_BAD_HANDLE;
    var ep = &endpoints[idx];
    if (!serviceSlotRunning(ep.service_slot)) return API_ERR_NOT_RUNNING;
    const slot_idx = deliveredRequestSlotById(ep, request_id) orelse return API_ERR_NOT_FOUND;
    var slot = &ep.queue[slot_idx];
    @memset(slot.response_payload[0..], 0);
    if (payload.len > 0) @memcpy(slot.response_payload[0..payload.len], payload);
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
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    clearMessageHeader(header);
    const idx = endpointForHandle(handle) orelse return API_ERR_BAD_HANDLE;
    var ep = &endpoints[idx];
    const slot_idx = respondedRequestSlotById(ep, request_id) orelse return 0;
    var slot = &ep.queue[slot_idx];
    const len: usize = @intCast(slot.response_len);
    fillMessageHeader(slot, header, slot.response_len, slot.response_status);
    if (out.len < len) {
        clearRequestSlot(slot);
        _ = ep.slots_available.release(1);
        return API_ERR_BUFFER_TOO_SMALL;
    }
    if (len > 0) @memcpy(out[0..len], slot.response_payload[0..len]);
    clearRequestSlot(slot);
    _ = ep.slots_available.release(1);
    return @intCast(len);
}

const ResponseWaitContext = struct {
    slot: *RequestSlot,
    request_id: u32,
};

fn responseWaitStillNeeded(raw: *anyopaque) bool {
    const ctx: *ResponseWaitContext = @ptrCast(@alignCast(raw));
    return ctx.slot.request_id == ctx.request_id and
        ctx.slot.state != .free and
        ctx.slot.state != .responded;
}

pub fn waitResponse(handle: u32, request_id: u32, timeout_ticks: u64) i32 {
    if (request_id == 0) return API_ERR_INVALID;
    const locked = lockRegistry();
    const idx = endpointForHandle(handle) orelse {
        unlockRegistry(locked);
        return API_ERR_BAD_HANDLE;
    };
    var ep = &endpoints[idx];
    if (!serviceSlotRunning(ep.service_slot)) {
        unlockRegistry(locked);
        return API_ERR_NOT_RUNNING;
    }
    if (respondedRequestSlotById(ep, request_id) != null) {
        unlockRegistry(locked);
        return API_OK;
    }
    const slot_idx = requestSlotById(ep, request_id) orelse {
        unlockRegistry(locked);
        return API_ERR_NOT_FOUND;
    };
    ep.completion_waits +%= 1;
    ep.completion_wait_rounds +%= 1;
    var wait_ctx = ResponseWaitContext{ .slot = &ep.queue[slot_idx], .request_id = request_id };
    const wait_result = wait_ctx.slot.response_available.waitUnlessReleasing(
        timeout_ticks,
        "service-response",
        responseWaitStillNeeded,
        &wait_ctx,
        releaseMutexForWait,
        &registry_lock,
    );

    const result_locked = lockRegistry();
    defer unlockRegistry(result_locked);
    const result_idx = endpointForHandle(handle) orelse return switch (wait_result) {
        .cancelled, .killed => API_ERR_NOT_RUNNING,
        else => API_ERR_BAD_HANDLE,
    };
    ep = &endpoints[result_idx];
    if (!serviceSlotRunning(ep.service_slot)) return API_ERR_NOT_RUNNING;
    if (wait_result == .timeout) {
        ep.timeouts +%= 1;
        ep.completion_timeouts +%= 1;
        return API_ERR_TIMEOUT;
    }
    if (wait_result == .cancelled or wait_result == .killed) return API_ERR_NOT_RUNNING;
    if (wait_result == .none or wait_result == .failed) return API_ERR_BUSY;
    if (respondedRequestSlotById(ep, request_id) != null) return API_OK;
    if (requestSlotById(ep, request_id) == null) return API_ERR_NOT_FOUND;
    return API_ERR_BUSY;
}

pub fn cancelRequest(handle: u32, request_id: u32) i32 {
    const locked = lockRegistry();
    defer unlockRegistry(locked);
    const idx = endpointForHandle(handle) orelse return API_ERR_BAD_HANDLE;
    var ep = &endpoints[idx];
    const slot_idx = requestSlotById(ep, request_id) orelse return API_ERR_NOT_FOUND;
    ep.drops +%= 1;
    ep.cancellations +%= 1;
    clearRequestSlot(&ep.queue[slot_idx]);
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
    endpoints = .{Endpoint{}} ** MAX_ENDPOINTS;
    next_endpoint_handle = 1;
    next_request_id = 1;
}

fn resetRegistryState() void {
    entries = .{Entry{}} ** MAX_SERVICES;
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

fn clearEndpointForSlot(slot: usize) void {
    var i: usize = 0;
    while (i < endpoints.len) : (i += 1) {
        if (endpoints[i].used and endpoints[i].service_slot == slot) {
            wakeEndpointWaiters(&endpoints[i], .cancelled);
            endpoints[i] = .{};
        }
    }
}

fn freeRequestSlot(ep: *const Endpoint) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == .free) return i;
    }
    return null;
}

fn queuedRequestSlot(ep: *const Endpoint) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == .queued) return i;
    }
    return null;
}

fn requestSlotById(ep: *const Endpoint, request_id: u32) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state != .free and ep.queue[i].request_id == request_id) return i;
    }
    return null;
}

fn deliveredRequestSlotById(ep: *const Endpoint, request_id: u32) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == .delivered and ep.queue[i].request_id == request_id) return i;
    }
    return null;
}

fn respondedRequestSlotById(ep: *const Endpoint, request_id: u32) ?usize {
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == .responded and ep.queue[i].request_id == request_id) return i;
    }
    return null;
}

fn countQueuedSlots(ep: *const Endpoint) u32 {
    return countSlotsByState(ep, .queued);
}

fn countDeliveredSlots(ep: *const Endpoint) u32 {
    return countSlotsByState(ep, .delivered);
}

fn countRespondedSlots(ep: *const Endpoint) u32 {
    return countSlotsByState(ep, .responded);
}

fn countUsedSlots(ep: *const Endpoint) u32 {
    var count: u32 = 0;
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state != .free) count += 1;
    }
    return count;
}

fn countSlotsByState(ep: *const Endpoint, state: RequestState) u32 {
    var count: u32 = 0;
    var i: usize = 0;
    while (i < ep.queue.len) : (i += 1) {
        if (ep.queue[i].state == state) count += 1;
    }
    return count;
}

fn noteQueueHighWater(ep: *Endpoint) void {
    const used = countUsedSlots(ep);
    if (used > ep.queue_high_water) ep.queue_high_water = used;
}

fn noteActiveWorkers(ep: *Endpoint) void {
    const active = countDeliveredSlots(ep);
    if (active > ep.max_active_workers) ep.max_active_workers = active;
}

fn clearRequestSlot(slot: *RequestSlot) void {
    _ = slot.response_available.close(.cancelled);
    slot.* = .{};
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

fn serviceSlotRunning(slot: usize) bool {
    return slot < entries.len and entries[slot].used and entries[slot].state == .running and entries[slot].instance_id != 0;
}

fn allocateEndpointHandle() u32 {
    const handle = next_endpoint_handle;
    next_endpoint_handle +%= 1;
    if (next_endpoint_handle == 0) next_endpoint_handle = 1;
    return if (handle == 0) 1 else handle;
}

fn allocateRequestId() u32 {
    const id = next_request_id;
    next_request_id +%= 1;
    if (next_request_id == 0 or next_request_id > 0x7FFF_FFFE) next_request_id = 1;
    return if (id == 0) 1 else id;
}

fn fillApiInfo(slot: usize, out: *ApiInfo, now_ticks: u64) void {
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
    if (endpointForSlot(slot)) |idx| {
        const ep = &endpoints[idx];
        out.handle = ep.handle;
        out.flags |= API_FLAG_ENDPOINT;
        out.flags |= API_FLAG_QUEUE_BACKED;
        const queued = countQueuedSlots(ep);
        const active = countDeliveredSlots(ep);
        const responded = countRespondedSlots(ep);
        if (queued != 0 or active != 0) out.flags |= API_FLAG_REQUEST_PENDING;
        if (responded != 0) out.flags |= API_FLAG_RESPONSE_PENDING;
        out.requests = ep.requests;
        out.responses = ep.responses;
        out.drops = ep.drops;
        out.queue_depth = @intCast(API_ENDPOINT_QUEUE_DEPTH);
        out.queue_used = countUsedSlots(ep);
        out.queue_high_water = ep.queue_high_water;
        out.active_workers = active;
        out.max_active_workers = ep.max_active_workers;
        out.open_handles = ep.open_handles;
        out.busy_rejections = ep.busy_rejections;
        out.timeouts = ep.timeouts;
        out.cancellations = ep.cancellations;
    }
}

fn fillApiDetail(slot: usize, out: *ApiDetail, now_ticks: u64) void {
    out.* = .{};
    if (slot >= entries.len or !entries[slot].used) return;
    const e = &entries[slot];
    fillApiInfo(slot, &out.info, now_ticks);
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
