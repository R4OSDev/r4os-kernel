// R4OS kernel - minimal service core contract.
//
// This file is the visible boundary between early kernel boot and the existing
// legacy service implementation. It intentionally contains only the technical
// service mechanics that may stay in the kernel: registry state, service
// status, endpoints, and request/response IPC.
//
// Policy stays out of here: SERVICES.R4S load/write, autostart planning,
// install/remove, user-facing diagnostics, and restart decisions belong in the
// external Service Manager.

const legacy = @import("services.zig");

pub const MAX_SERVICES = legacy.MAX_SERVICES;
pub const MAX_NAME = legacy.MAX_NAME;
pub const MAX_PATH = legacy.MAX_PATH;
pub const MAX_ARGS = legacy.MAX_ARGS;
pub const MAX_DESCRIPTION = legacy.MAX_DESCRIPTION;
pub const MAX_ERROR = legacy.MAX_ERROR;

pub const OK = legacy.OK;
pub const ERR_INVALID = legacy.ERR_INVALID;
pub const ERR_FULL = legacy.ERR_FULL;
pub const ERR_DUPLICATE = legacy.ERR_DUPLICATE;
pub const ERR_NOT_FOUND = legacy.ERR_NOT_FOUND;

pub const API_MAGIC = legacy.API_MAGIC;
pub const API_VERSION = legacy.API_VERSION;
pub const API_HEADER_SIZE = legacy.API_HEADER_SIZE;
pub const API_MAX_PAYLOAD = legacy.API_MAX_PAYLOAD;
pub const API_ENDPOINT_QUEUE_DEPTH = legacy.API_ENDPOINT_QUEUE_DEPTH;
pub const API_OK = legacy.API_OK;
pub const API_ERR_INVALID = legacy.API_ERR_INVALID;
pub const API_ERR_NOT_FOUND = legacy.API_ERR_NOT_FOUND;
pub const API_ERR_NOT_RUNNING = legacy.API_ERR_NOT_RUNNING;
pub const API_ERR_NO_ENDPOINT = legacy.API_ERR_NO_ENDPOINT;
pub const API_ERR_PAYLOAD_TOO_LARGE = legacy.API_ERR_PAYLOAD_TOO_LARGE;
pub const API_ERR_BUFFER_TOO_SMALL = legacy.API_ERR_BUFFER_TOO_SMALL;
pub const API_ERR_BUSY = legacy.API_ERR_BUSY;
pub const API_ERR_TIMEOUT = legacy.API_ERR_TIMEOUT;
pub const API_ERR_BAD_HANDLE = legacy.API_ERR_BAD_HANDLE;
pub const API_ERR_FULL = legacy.API_ERR_FULL;
pub const API_ERR_BAD_OP = legacy.API_ERR_BAD_OP;
pub const API_ERR_DUPLICATE = legacy.API_ERR_DUPLICATE;

pub const API_FLAG_ENDPOINT = legacy.API_FLAG_ENDPOINT;
pub const API_FLAG_REQUEST_PENDING = legacy.API_FLAG_REQUEST_PENDING;
pub const API_FLAG_RESPONSE_PENDING = legacy.API_FLAG_RESPONSE_PENDING;
pub const API_FLAG_QUEUE_BACKED = legacy.API_FLAG_QUEUE_BACKED;

pub const API_STATE_EMPTY = legacy.API_STATE_EMPTY;
pub const API_STATE_STOPPED = legacy.API_STATE_STOPPED;
pub const API_STATE_STARTING = legacy.API_STATE_STARTING;
pub const API_STATE_RUNNING = legacy.API_STATE_RUNNING;
pub const API_STATE_STOPPING = legacy.API_STATE_STOPPING;
pub const API_STATE_FAILED = legacy.API_STATE_FAILED;
pub const API_STATE_DISABLED = legacy.API_STATE_DISABLED;

pub const API_START_MANUAL = legacy.API_START_MANUAL;
pub const API_START_AUTO = legacy.API_START_AUTO;
pub const API_START_DISABLED = legacy.API_START_DISABLED;

pub const State = legacy.State;
pub const StartMode = legacy.StartMode;
pub const Entry = legacy.Entry;
pub const ApiInfo = legacy.ApiInfo;
pub const ApiDetail = legacy.ApiDetail;
pub const ApiMessageHeader = legacy.ApiMessageHeader;
pub const PerformanceSummary = legacy.PerformanceSummary;

pub fn init() void {
    legacy.init();
}

pub fn register(name: []const u8, path: []const u8, args: []const u8, start_mode: StartMode) i32 {
    return legacy.register(name, path, args, start_mode);
}

pub fn registerWithDescription(name: []const u8, path: []const u8, args: []const u8, start_mode: StartMode, description: []const u8) i32 {
    return legacy.registerWithDescription(name, path, args, start_mode, description);
}

pub fn unregister(name: []const u8) i32 {
    return legacy.unregister(name);
}

pub fn setState(name: []const u8, state: State, instance_id: u32, exit_code: i32, error_text: []const u8) i32 {
    return legacy.setState(name, state, instance_id, exit_code, error_text);
}

pub fn markStarting(name: []const u8) i32 {
    return legacy.markStarting(name);
}

pub fn markRunning(name: []const u8, instance_id: u32, start_tick: u64) i32 {
    return legacy.markRunning(name, instance_id, start_tick);
}

pub fn markStopping(name: []const u8) i32 {
    return legacy.markStopping(name);
}

pub fn markStopped(name: []const u8, exit_code: i32) i32 {
    return legacy.markStopped(name, exit_code);
}

pub fn markFailed(name: []const u8, exit_code: i32, error_text: []const u8) i32 {
    return legacy.markFailed(name, exit_code, error_text);
}

pub fn entryAt(index: usize) ?Entry {
    return legacy.entryAt(index);
}

pub fn entryByName(name: []const u8) ?Entry {
    return legacy.entryByName(name);
}

pub fn countUsed() usize {
    return legacy.countUsed();
}

pub fn performanceSummary() PerformanceSummary {
    return legacy.performanceSummary();
}

pub fn apiInfoAt(index: u32, out: *ApiInfo, now_ticks: u64) i32 {
    return legacy.apiInfoAt(index, out, now_ticks);
}

pub fn apiDetailAt(index: u32, out: *ApiDetail, now_ticks: u64) i32 {
    return legacy.apiDetailAt(index, out, now_ticks);
}

pub fn apiStatus(name: []const u8, out: *ApiInfo, now_ticks: u64) i32 {
    return legacy.apiStatus(name, out, now_ticks);
}

pub fn apiDetailByName(name: []const u8, out: *ApiDetail, now_ticks: u64) i32 {
    return legacy.apiDetailByName(name, out, now_ticks);
}

pub fn apiOpen(name: []const u8, out: *ApiInfo, now_ticks: u64) i32 {
    return legacy.apiOpen(name, out, now_ticks);
}

pub fn apiClose(handle: u32) i32 {
    return legacy.apiClose(handle);
}

pub fn registerEndpoint(name: []const u8, instance_id: u32, flags: u32, out: *ApiInfo, now_ticks: u64) i32 {
    return legacy.registerEndpoint(name, instance_id, flags, out, now_ticks);
}

pub fn unregisterEndpoint(handle: u32) i32 {
    return legacy.unregisterEndpoint(handle);
}

pub fn endpointPoll(handle: u32) i32 {
    return legacy.endpointPoll(handle);
}

// 0.56.19: Blockierendes Endpoint-Warten (siehe services.endpointWait).
pub fn endpointWait(handle: u32, timeout_ticks: u64) i32 {
    return legacy.endpointWait(handle, timeout_ticks);
}

pub fn submitRequest(handle: u32, client_id: u32, op: u16, payload: []const u8) i32 {
    return legacy.submitRequest(handle, client_id, op, payload);
}

pub fn submitRequestWait(handle: u32, client_id: u32, op: u16, payload: []const u8, timeout_ticks: u64) i32 {
    return legacy.submitRequestWait(handle, client_id, op, payload, timeout_ticks);
}

pub fn recvRequest(handle: u32, header: *ApiMessageHeader, out: []u8) i32 {
    return legacy.recvRequest(handle, header, out);
}

pub fn reply(handle: u32, request_id: u32, status: i32, payload: []const u8) i32 {
    return legacy.reply(handle, request_id, status, payload);
}

pub fn takeResponse(handle: u32, request_id: u32, header: *ApiMessageHeader, out: []u8) i32 {
    return legacy.takeResponse(handle, request_id, header, out);
}

pub fn waitResponse(handle: u32, request_id: u32, timeout_ticks: u64) i32 {
    return legacy.waitResponse(handle, request_id, timeout_ticks);
}

pub fn cancelRequest(handle: u32, request_id: u32) i32 {
    return legacy.cancelRequest(handle, request_id);
}

pub fn stateCode(state: State) u32 {
    return legacy.stateCode(state);
}

pub fn startModeCode(start_mode: StartMode) u32 {
    return legacy.startModeCode(start_mode);
}
