const native_tasks = @import("../sched/task.zig");
const diag_screen = @import("../kernel/diag_screen.zig");
const block_dispatch = @import("block_dispatch.zig");
const block_split = @import("block_split.zig");
const access = @import("access_runtime.zig");
const drive = @import("../fs/drive.zig");
const heap = @import("../memory/heap.zig");
const owner_locks = @import("../memory/owner_locks.zig");
const k = @import("../kernel/log.zig");
const std = @import("std");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const sched_task = @import("../sched/task.zig");
const task_context = @import("../sched/task_context.zig");
const timer = @import("../kernel/timer.zig");

const MAX_DEVICES: usize = 16;
pub const MAX_REQUEST_QUEUE_DEPTH: usize = 16;

pub const ReadFn = *const fn (ctx: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool;
pub const WriteFn = *const fn (ctx: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool;
pub const FlushFn = *const fn (ctx: ?*anyopaque) bool;
pub const AsyncCompleteFn = *const fn (handle: u64, result: i32, bytes: u32) callconv(.c) void;

pub const ASYNC_RESULT_OK: i32 = 0;
pub const ASYNC_RESULT_ERROR: i32 = -1;
pub const ASYNC_RESULT_CANCELLED: i32 = -2;
pub const ASYNC_RESULT_TIMEOUT: i32 = -3;
pub const ASYNC_RESULT_RESET: i32 = -4;
pub const ASYNC_RESULT_SHUTDOWN: i32 = -5;

pub const CANCEL_REASON_TIMEOUT: u32 = 1;
pub const CANCEL_REASON_CALLER: u32 = 2;
pub const CANCEL_REASON_RESET: u32 = 3;
pub const CANCEL_REASON_SHUTDOWN: u32 = 4;

pub const AsyncRequest = struct {
    handle: u64,
    kind: RequestKind,
    lba: u64,
    sectors: u16,
    buffer: ?[*]u8,
    const_buffer: ?[*]const u8,
    buffer_len: usize,
    complete: AsyncCompleteFn,
};

// `submit` is nonblocking. Zero transfers request ownership to the backend;
// every accepted request must publish exactly one completion. A nonzero
// result rejects ownership unless the backend already completed inline.
pub const AsyncSubmitFn = *const fn (ctx: ?*anyopaque, request: *const AsyncRequest) i32;
pub const AsyncCancelFn = *const fn (ctx: ?*anyopaque, handle: u64, reason: u32) i32;
// A successful reset proves that no old command can access its buffer again.
pub const AsyncResetFn = *const fn (ctx: ?*anyopaque, reason: u32) i32;

pub const Bus = enum {
    unknown,
    ata,
    ahci,
    nvme,
    usb,
    ram,
    virtio,
};

pub const State = enum(u8) {
    registered,
    active,
    busy,
    recovering,
    failed,
};

pub const Source = enum {
    builtin,
    preload,
    disk,
};

pub const RequestKind = enum(u8) {
    none,
    read,
    write,
    flush,
};

pub const RequestSnapshot = extern struct {
    id: u64 = 0,
    kind: RequestKind = .none,
    lba: u64 = 0,
    sectors: u16 = 0,
};

pub const SenseSnapshot = extern struct {
    valid: bool = false,
    opcode: u8 = 0,
    key: u8 = 0,
    asc: u8 = 0,
    ascq: u8 = 0,
};

const RequestState = enum(u8) {
    free,
    queued,
    active,
    completed,
};

const RequestSlot = extern struct {
    state: RequestState = .free,
    id: u64 = 0,
    kind: RequestKind = .none,
    lba: u64 = 0,
    sectors: u16 = 0,
    buffer: ?[*]u8 = null,
    const_buffer: ?[*]const u8 = null,
    buffer_len: usize = 0,
    ok: bool = false,
    err: Error = .none,
    submit_tick: u64 = 0,
    start_tick: u64 = 0,
    complete_tick: u64 = 0,
    // Once a runtime request has entered the backend, its buffer must remain
    // alive until finishRequest observes that the synchronous callback has
    // returned. Bounce ownership may transfer to a late completion. A direct
    // resident borrower instead remains blocked until that callback returns.
    buffer_ownership: block_dispatch.BufferOwnership = .none,
    timeout_requested: bool = false,
    caller_detached: bool = false,
    cancel_requested: bool = false,
    completion_override: Error = .none,
    backend_handle: u64 = 0,
    completion_latch: block_dispatch.CompletionLatch = .{},
    execution_mode: ExecutionMode = .boot_inline,
};

const RequestExecution = extern struct {
    slot_index: usize,
    id: u64,
    kind: RequestKind,
    lba: u64,
    sectors: u16,
    buffer: ?[*]u8,
    const_buffer: ?[*]const u8,
    buffer_len: usize,
    buffer_ownership: block_dispatch.BufferOwnership,
    start_tick: u64,
    mode: ExecutionMode,
    backend_handle: u64,
};

const RequestResult = extern struct {
    ok: bool = false,
    err: Error = .none,
    buffer_detached: bool = false,
};

const ExecutionMode = enum(u8) {
    boot_inline,
    runtime_worker,
};

pub const Error = enum(u8) {
    none,
    busy,
    invalid_request,
    request_too_large,
    out_of_range,
    buffer_too_small,
    no_writer,
    timeout,
    backend_read,
    backend_write,
    backend_flush,
    cancelled,
    reset,
    shutdown,
};

/// Exact committed prefix of a logical write.  Filesystem/cache callers use
/// this to invalidate only sectors that definitely reached the backend when
/// a later backend-sized chunk fails.
pub const TransferResult = struct {
    sectors_completed: u16 = 0,
    err: Error = .none,
};

extern fn r4os_rust_block_async_submission(u8, bool) callconv(.c) bool;
extern fn r4os_rust_block_backend_error(u8) callconv(.c) u8;
extern fn r4os_rust_block_validate(u32, u64, u64, u16, usize) callconv(.c) u8;
extern fn r4os_rust_block_classify(u8, usize, i32, u32, *RequestResult) callconv(.c) void;
extern fn r4os_rust_block_snapshot(*const RequestSlot, *RequestSnapshot) callconv(.c) void;
extern fn r4os_rust_block_select(*const [MAX_REQUEST_QUEUE_DEPTH]RequestSlot, u8, u64) callconv(.c) usize;
extern fn r4os_rust_block_oldest(*const [MAX_REQUEST_QUEUE_DEPTH]RequestSlot, usize, *RequestSnapshot) callconv(.c) void;
comptime {
    if (@sizeOf(RequestSlot) != 144 or @offsetOf(RequestSlot, "backend_handle") != 104 or @offsetOf(RequestSlot, "completion_latch") != 112 or @sizeOf(RequestSnapshot) != 32 or @sizeOf(RequestResult) != 3) @compileError("Native block request C layout differs");
}

const SlotFinish = extern struct { accepted: bool = false, caller_detached: bool = false, completion_override: Error = .none, result_ok: bool = false, result_error: Error = .none, detached_buffer: ?[*]u8 = null, detached_len: usize = 0 };
const SlotAbort = extern struct { action: u8 = 0, kind: RequestKind = .none, first_timeout: bool = false, first_cancel: bool = false, first_detach: bool = false, buffer_detached: bool = false };
extern fn r4os_rust_block_slot_enqueue(*RequestSlot, *const RequestExecution, u64, u64) callconv(.c) void;
extern fn r4os_rust_block_slot_start(*RequestSlot, usize, u8, u64, bool, u64, *RequestExecution) callconv(.c) bool;
extern fn r4os_rust_block_slot_finish(*RequestSlot, *const RequestExecution, bool, u8, u64, *SlotFinish) callconv(.c) bool;
extern fn r4os_rust_block_slot_abort(*RequestSlot, u8, *SlotAbort, *RequestExecution, usize) callconv(.c) void;
extern fn r4os_rust_block_slot_clear(*RequestSlot) callconv(.c) void;
extern fn r4os_rust_block_slot_reset(*RequestSlot, usize, *RequestExecution, *bool) callconv(.c) u8;
extern fn r4os_rust_block_execution(usize, *const RequestSlot, *RequestExecution) callconv(.c) void;
comptime {
    if (@sizeOf(RequestExecution) != 96 or @offsetOf(RequestExecution, "backend_handle") != 88 or @sizeOf(SlotFinish) != 24 or @sizeOf(SlotAbort) != 6) @compileError("Native slot transition C layout differs");
}

pub const Stats = extern struct {
    next_request_id: u64 = 0,
    completions: u64 = 0,
    busy_rejections: u64 = 0,
    timeout_failures: u64 = 0,
    active_request: RequestSnapshot = .{},
    last_request: RequestSnapshot = .{},
    queued_requests: u64 = 0,
    dequeued_requests: u64 = 0,
    queue_full_waits: u64 = 0,
    queue_full_rejections: u64 = 0,
    completion_waits: u64 = 0,
    completion_timeouts: u64 = 0,
    completion_total_ticks: u64 = 0,
    completion_max_ticks: u64 = 0,
    completion_last_ticks: u64 = 0,
    completion_signals: u64 = 0,
    worker_requests: u64 = 0,
    worker_completions: u64 = 0,
    boot_inline_requests: u64 = 0,
    boot_inline_completions: u64 = 0,
    queue_high_water: u32 = 0,
    read_ops: u64 = 0,
    read_sectors: u64 = 0,
    read_failures: u64 = 0,
    write_ops: u64 = 0,
    write_sectors: u64 = 0,
    write_failures: u64 = 0,
    flush_ops: u64 = 0,
    flush_failures: u64 = 0,
    backend_recoveries: u64 = 0,
    backend_recovery_failures: u64 = 0,
    direct_requests: u64 = 0,
    direct_bytes: u64 = 0,
    bounce_allocations: u64 = 0,
    bounce_bytes: u64 = 0,
    bounce_copy_bytes: u64 = 0,
    direct_timeout_waits: u64 = 0,
    async_submissions: u64 = 0,
    async_completions: u64 = 0,
    async_cancel_requests: u64 = 0,
    async_resets: u64 = 0,
    duplicate_completions: u64 = 0,
    late_completions: u64 = 0,
    in_flight_high_water: u32 = 0,
    last_sense: SenseSnapshot = .{},
    last_error: Error = .none,
};

pub const RuntimeSummary = extern struct {
    worker_started: u32 = 0,
    worker_task_id: u32 = 0,
    worker_wakeups: u64 = 0,
    worker_runs: u64 = 0,
    worker_idle_waits: u64 = 0,
    worker_queue_scans: u64 = 0,
    worker_runtime_requests: u64 = 0,
    worker_runtime_completions: u64 = 0,
    boot_inline_requests: u64 = 0,
    boot_inline_completions: u64 = 0,
    completion_signals: u64 = 0,
    controller_count: u32 = 0,
    worker_count: u32 = 0,
    worker_start_failures: u64 = 0,
    worker_parallel_active: u32 = 0,
    worker_parallel_active_max: u32 = 0,
    direct_requests: u64 = 0,
    direct_bytes: u64 = 0,
    bounce_allocations: u64 = 0,
    bounce_bytes: u64 = 0,
    bounce_copy_bytes: u64 = 0,
    direct_timeout_waits: u64 = 0,
    async_submissions: u64 = 0,
    async_completions: u64 = 0,
    async_cancel_requests: u64 = 0,
    async_resets: u64 = 0,
    duplicate_completions: u64 = 0,
    late_completions: u64 = 0,
    in_flight: u32 = 0,
    in_flight_high_water: u32 = 0,
};

extern fn r4os_rust_block_runtime_reset() callconv(.c) void;
extern fn r4os_rust_block_runtime_snapshot(*RuntimeSummary) callconv(.c) void;
extern fn r4os_rust_block_runtime_change(u8, u8, u64) callconv(.c) u64;
extern fn r4os_rust_block_runtime_active(bool) callconv(.c) void;
extern fn r4os_rust_block_runtime_execution(u8, bool) callconv(.c) void;
extern fn r4os_rust_block_next_handle(u8, usize) callconv(.c) u64;
extern fn r4os_rust_block_stats_change(*Stats, u8, u8, u64) callconv(.c) u64;
extern fn r4os_rust_block_stats_snapshot(*const Stats, *Stats) callconv(.c) void;
extern fn r4os_rust_block_stats_reset(*Stats) callconv(.c) void;
extern fn r4os_rust_block_stats_request(*Stats, bool, *const RequestSnapshot) callconv(.c) void;
extern fn r4os_rust_block_stats_sense(*Stats, u8, u8, u8, u8) callconv(.c) void;
comptime {
    if (@sizeOf(Stats) != 408 or @sizeOf(RuntimeSummary) != 208 or @sizeOf(SenseSnapshot) != 5) @compileError("Native block accounting C layout differs");
}
pub fn statsSnapshot(device: *const Device) Stats {
    var out: Stats = undefined;
    r4os_rust_block_stats_snapshot(&device.stats, &out);
    return out;
}

pub const Device = struct {
    // Assigned by the registry; never supplied by a driver or reused after retirement.
    access_ref: access.DeviceRef = .{ .slot = 0, .generation = 0 },
    name: []const u8,
    // Hardware-provided text when available; distinct from the registry name.
    model: []const u8 = "",
    driver: []const u8 = "unknown",
    bus: Bus = .unknown,
    controller: []const u8 = "unknown",
    port: u8 = 0,
    sector_size: u32,
    sector_count: u64,
    max_sectors_per_request: u16 = 0,
    queue_depth: u16 = 1,
    timeout_ticks: u64 = 0,
    removable: bool = false,
    writable: bool = false,
    // The backend already classifies transport failures, performs recovery
    // and replays an idempotent command with an exact bounded policy.
    // Higher cache layers must not multiply that retry sequence.
    owns_transport_retry: bool = false,
    source: Source = .builtin,
    owner_id: u32 = 0,
    ctx: ?*anyopaque,
    read_fn: ReadFn,
    write_fn: ?WriteFn = null,
    flush_fn: ?FlushFn = null,
    async_submit_fn: ?AsyncSubmitFn = null,
    async_cancel_fn: ?AsyncCancelFn = null,
    async_reset_fn: ?AsyncResetFn = null,
    state: State = .registered,
    resetting: bool = false,
    slot_index: u8 = 0,
    stats: Stats = .{},
    active_executions: u32 = 0,
    controller_lane: u8 = block_dispatch.no_lane,
    request_slots: [MAX_REQUEST_QUEUE_DEPTH]RequestSlot = .{RequestSlot{}} ** MAX_REQUEST_QUEUE_DEPTH,
    slot_available: sync.WaitQueue = sync.WaitQueue.init(),
    completion_available: sync.WaitQueue = sync.WaitQueue.init(),
    queue_lock: sync.Mutex = sync.Mutex.initClass("block-device", sync.LockRank.block_device, .sleepable),
};

// Native registry owns all slot identity, retirement and pin metadata.
const DeviceSlot = extern struct { device: *Device, index: usize, retire_generation: u64, reference: access.DeviceRef, pins: u32, used: bool, retiring: bool, lane: u8 };
const DevicePin = extern struct { index: usize, device: *Device, unwind: task_context.UnwindToken, active: bool = true };
const RegistryText = extern struct { data: [*]const u8, len: usize };
const Registration = extern struct { name: RegistryText, model: RegistryText, driver: RegistryText, controller: RegistryText, sectors: u64, sector_bytes: u32, queue_depth: u16, max_sectors: u16, bus: u8, writable: bool, owns_retry: bool, async_submit: bool };
extern fn r4os_rust_block_registry_init(*const [MAX_DEVICES]*Device) callconv(.c) void;
extern fn r4os_rust_block_registry_register(*const Registration, *const Device, *const fn (*Device, *const Device, *const access.DeviceRef, u8) callconv(.c) void, *DeviceSlot) callconv(.c) bool;
extern fn r4os_rust_block_registry_slot(usize, u8, u64, bool, *DeviceSlot) callconv(.c) bool;
extern fn r4os_rust_block_registry_retire(usize, u8, *DeviceSlot) callconv(.c) bool;
extern fn r4os_rust_block_registry_pin(usize, *DevicePin) callconv(.c) bool;
extern fn r4os_rust_block_registry_unpin(*DevicePin) callconv(.c) void;
extern fn r4os_rust_block_registry_find([*]const u8, usize) callconv(.c) usize;
extern fn r4os_rust_block_registry_query(u8, u8) callconv(.c) usize;
extern fn r4os_rust_block_registry_exclude(u8, bool) callconv(.c) bool;
extern fn r4os_rust_block_registry_identity(usize, *NativeIdentity) callconv(.c) bool;
extern fn r4os_rust_block_registry_limits(usize, *DirectWriteLimits) callconv(.c) bool;
fn registryText(s: []const u8) RegistryText {
    return .{ .data = s.ptr, .len = s.len };
}
fn slotView(index: usize, mode: u8, generation: u64, locked: bool) ?DeviceSlot {
    var out: DeviceSlot = undefined;
    return if (r4os_rust_block_registry_slot(index, mode, generation, locked, &out)) out else null;
}
fn initializeRegistryDevice(out: *Device, input: *const Device, reference: *const access.DeviceRef, lane: u8) callconv(.c) void {
    out.* = input.*;
    out.access_ref = reference.*;
    _ = changeDeviceState(out, 0, .none, .none, 0);
    out.slot_index = @intCast(reference.slot);
    out.controller_lane = lane;
    var view: NativeExecutionView = undefined;
    r4os_zig_block_execution_view(out, &view);
    r4os_rust_block_sync_init(&view);
}
comptime {
    if (@sizeOf(Registration) != 88 or @sizeOf(DeviceSlot) != 48 or @sizeOf(DevicePin) != 40 or @sizeOf(NativeIdentity) != 160 or @sizeOf(DirectWriteLimits) != 24) @compileError("Native block registry C layout differs");
}

// A prepared unregister owns the retiring admission barrier until it is
// either committed or cancelled. Index plus retirement generation avoids
// exposing DeviceSlot and rejects copied stale tokens after cancel/reuse.
pub const UnregisterToken = extern struct {
    index: usize = 0,
    generation: u64 = 0,
    active: bool = false,
};

// Device addresses are stable for the complete lifetime of one registration.
// Unregister leaves a tombstone instead of shifting later Device values (and
// their embedded WaitQueues/Mutex); register may reuse only a quiescent slot.
// Resident request/device leaf storage only; no Zig registry metadata.
var registry_device_storage: [MAX_DEVICES]Device = undefined;
// Session-wide media policy, applied before physical volumes are admitted.
// Hardware/USB HID owners are independent of block-device visibility.

pub fn init() void {
    access.initForBoot();
    var addresses: [MAX_DEVICES]*Device = undefined;
    for (&addresses, 0..) |*address, i| address.* = &registry_device_storage[i];
    r4os_rust_block_registry_init(&addresses);
    r4os_rust_block_workers_init();
    r4os_rust_block_runtime_reset();
}

pub fn initRuntimeWorker() bool {
    return r4os_rust_block_workers_start();
}
pub fn runtimeWorkerReady() bool {
    return r4os_rust_block_workers_ready();
}
pub fn runtimeWorkerSummary() RuntimeSummary {
    var out: RuntimeSummary = undefined;
    r4os_rust_block_workers_summary(&out);
    return out;
}
pub fn register(device: Device) ?usize {
    const info = Registration{ .name = registryText(device.name), .model = registryText(device.model), .driver = registryText(device.driver), .controller = registryText(device.controller), .sectors = device.sector_count, .sector_bytes = device.sector_size, .queue_depth = device.queue_depth, .max_sectors = device.max_sectors_per_request, .bus = @intFromEnum(device.bus), .writable = device.writable, .owns_retry = device.owns_transport_retry, .async_submit = device.async_submit_fn != null };
    var out: DeviceSlot = undefined;
    if (!r4os_rust_block_register(&info, &device, initializeRegistryDevice, &out)) return null;
    return out.index;
}

pub fn unregister(index: usize) bool {
    return r4os_rust_block_unregister(index);
}
// Stop new admissions and prove the complete device lifetime is quiescent,
// but leave the registration intact. Callers may prepare several devices and
// cancel all of them without having partially removed an owner.
pub fn prepareUnregister(index: usize) ?UnregisterToken {
    var out: UnregisterToken = undefined;
    return if (r4os_rust_block_retirement_prepare(index, &out)) out else null;
}
pub fn commitUnregister(token: *UnregisterToken) bool {
    return r4os_rust_block_retirement_commit(token);
}
pub fn cancelUnregister(token: *UnregisterToken) bool {
    return r4os_rust_block_retirement_cancel(token);
}

pub fn findByName(name: []const u8) ?usize {
    const index = r4os_rust_block_registry_find(name.ptr, name.len);
    return if (index == std.math.maxInt(usize)) null else index;
}

pub fn count() usize {
    return r4os_rust_block_registry_query(0, 0);
}

pub fn excludeBusBeforeMount(bus: Bus) bool {
    return r4os_rust_block_exclude(@intFromEnum(bus));
}
pub fn isBusVisible(bus: Bus) bool {
    return r4os_rust_block_registry_query(2, @intFromEnum(bus)) != 0;
}

pub fn slotCount() usize {
    return r4os_rust_block_registry_query(1, 0);
}

pub fn maxDevices() usize {
    return MAX_DEVICES;
}

const NativeIdentity = extern struct {
    reference: access.DeviceRef,
    sectors: u64,
    sector_bytes: u32,
    writable: bool,
    bus: u8,
    name: [32]u8 = .{0} ** 32,
    model: [64]u8 = .{0} ** 64,
    driver: [32]u8 = .{0} ** 32,
};

pub const Identity = struct {
    reference: access.DeviceRef,
    sectors: u64,
    sector_bytes: u32,
    writable: bool,
    bus: Bus,
    name: [32]u8 = .{0} ** 32,
    model: [64]u8 = .{0} ** 64,
    driver: [32]u8 = .{0} ** 32,
};

// Copy under the registry owner; no pointer into a driver-owned string or a
// reusable DeviceSlot crosses the public inventory boundary.
pub fn identity(index: usize) ?Identity {
    var out: NativeIdentity = undefined;
    if (!r4os_rust_block_registry_identity(index, &out)) return null;
    return .{ .reference = out.reference, .sectors = out.sectors, .sector_bytes = out.sector_bytes, .writable = out.writable, .bus = @enumFromInt(out.bus), .name = out.name, .model = out.model, .driver = out.driver };
}

pub fn get(index: usize) ?*const Device {
    return (slotView(index, 1, 0, false) orelse return null).device;
}

pub fn queueUsed(index: usize) u32 {
    return r4os_rust_block_device_operation(index, 0, 0, 0, 0, 0);
}
pub fn beginBackendRecovery(index: usize) void {
    _ = r4os_rust_block_device_operation(index, 1, 0, 0, 0, 0);
}
pub fn finishBackendRecovery(index: usize, ok: bool) void {
    _ = r4os_rust_block_device_operation(index, if (ok) 2 else 3, 0, 0, 0, 0);
} // A successful backend reset is the only boundary that may retire an active
// asynchronous request without waiting for its physical completion. The
// backend promises that old commands can no longer touch buffers before the
// callback returns; handles are invalidated before waiters or slots are freed.
pub fn reset(index: usize, reason: u32) bool {
    return r4os_rust_block_reset(index, reason);
}

pub fn recordSense(index: usize, opcode: u8, key: u8, asc: u8, ascq: u8) void {
    _ = r4os_rust_block_device_operation(index, 4, opcode, key, asc, ascq);
}
pub fn busLabel(bus: Bus) []const u8 {
    var out: RegistryText = undefined;
    r4os_rust_block_label(0, @intFromEnum(bus), &out);
    return out.data[0..out.len];
}
pub fn stateLabel(state: State) []const u8 {
    var out: RegistryText = undefined;
    r4os_rust_block_label(1, @intFromEnum(state), &out);
    return out.data[0..out.len];
}
pub fn errorLabel(err: Error) []const u8 {
    var out: RegistryText = undefined;
    r4os_rust_block_label(2, @intFromEnum(err), &out);
    return out.data[0..out.len];
}
pub fn sourceLabel(source: Source) []const u8 {
    var out: RegistryText = undefined;
    r4os_rust_block_label(3, @intFromEnum(source), &out);
    return out.data[0..out.len];
}
pub fn read(index: usize, lba: u64, sectors: u16, out: []u8) bool {
    return readWithPolicy(index, lba, sectors, out, false);
}

/// Kernel-only direct I/O. The caller proves that `out` is resident,
/// owner-fixed and alive until this synchronous call returns. Unlike the
/// general API, an active timeout cannot detach this borrowed buffer.
pub fn readDirect(index: usize, lba: u64, sectors: u16, out: []u8) bool {
    return readWithPolicy(index, lba, sectors, out, true);
}

/// Test-profile-only, read-only proof that one canonical NVMe request crosses
/// the runtime block worker and asynchronous backend boundary. The NVME.R4D
/// emits the companion `[NVMEIRQ]` marker only after an MSI/MSI-X-triggered CQ
/// drain with no polling fallback.
pub fn nvmeInterruptAcceptanceProbe() bool {
    return r4os_rust_block_nvme_probe();
}

fn readWithPolicy(index: usize, lba: u64, sectors: u16, out: []u8, trusted_resident: bool) bool {
    return r4os_rust_block_read(index, lba, sectors, out.ptr, out.len, trusted_resident);
}

pub fn write(index: usize, lba: u64, sectors: u16, data: []const u8) bool {
    const result = writeWithProgress(index, lba, sectors, data);
    return result.err == .none and result.sectors_completed == sectors;
}

pub fn writeWithProgress(index: usize, lba: u64, sectors: u16, data: []const u8) TransferResult {
    return writeWithProgressPolicy(index, lba, sectors, data, false);
}

/// Kernel-only direct I/O with the same resident, owner-fixed lifetime
/// requirement as readDirect.
pub fn writeDirect(index: usize, lba: u64, sectors: u16, data: []const u8) bool {
    const result = writeDirectWithProgress(index, lba, sectors, data);
    return result.err == .none and result.sectors_completed == sectors;
}

pub fn writeDirectWithProgress(index: usize, lba: u64, sectors: u16, data: []const u8) TransferResult {
    return writeWithProgressPolicy(index, lba, sectors, data, true);
}

fn writeWithProgressPolicy(index: usize, lba: u64, sectors: u16, data: []const u8, trusted_resident: bool) TransferResult {
    var out: NativeTransfer = undefined;
    r4os_rust_block_write(index, lba, sectors, data.ptr, data.len, trusted_resident, &out);
    return .{ .sectors_completed = out.sectors_completed, .err = @enumFromInt(out.err) };
}

pub const direct_write_batch_limit: usize = 4;
pub const DirectWriteLimits = extern struct {
    reference: access.DeviceRef,
    max_sectors: u16,
    owns_transport_retry: bool,
};

/// Copy capabilities while the registration owner is held; callers do not
/// keep a pointer to a reusable device slot across cache lock drops.
pub fn directWriteLimits(index: usize) ?DirectWriteLimits {
    var out: DirectWriteLimits = undefined;
    return if (r4os_rust_block_registry_limits(index, &out)) out else null;
}

pub const DirectWrite = struct {
    lba: u64,
    sectors: u16,
    data: []const u8,
    attempted: bool = false,
    ok: bool = false,
};
pub const DirectWriteBatchResult = struct { pending_max: usize = 0 };

/// Resident buffers are borrowed until every published request completes.
/// A generation-bound device pin/unwind spans admission, backpressure and
/// all waits. No buffers detach, even when a timeout/reset is classified.
/// The existing queue still orders flush barriers; only disjoint
/// writes explicitly admitted by the caller may overlap.
pub fn writeBatchDirect(index: usize, expected: access.DeviceRef, writes: []DirectWrite, parallel: bool) DirectWriteBatchResult {
    for (writes) |*job| {
        job.ok = false;
        job.attempted = false;
    }
    if (writes.len == 0 or writes.len > direct_write_batch_limit) return .{};
    var jobs: [direct_write_batch_limit]NativeWrite = undefined;
    for (writes, 0..) |job, i| jobs[i] = .{ .lba = job.lba, .sectors = job.sectors, .data = job.data.ptr, .len = job.data.len, .attempted = false, .ok = false };
    const pending = r4os_rust_block_write_batch(index, &expected, &jobs, writes.len, parallel);
    for (writes, 0..) |*job, i| {
        job.attempted = jobs[i].attempted;
        job.ok = jobs[i].ok;
    }
    return .{ .pending_max = pending };
}
pub fn flush(index: usize) bool {
    return r4os_rust_block_flush(index);
}

fn useAsyncSubmission(mode: ExecutionMode, has_submit: bool) bool {
    return r4os_rust_block_async_submission(@intFromEnum(mode), has_submit);
}

pub fn asyncBackendComplete(handle: u64, result: i32, bytes: u32) callconv(.c) void {
    r4os_rust_block_complete(handle, result, bytes);
}

// Storage-wait watchdog (0.60.20): a device registered without a timeout
// used to park the requester with WAIT_FOREVER.  A single wedged request
// (worker stuck in the driver, lost completion) then froze the whole
// system, because the requester holds the FS request path while it sleeps.
// Now every wait runs in bounded observation slices.  A queued request can
// be cancelled safely when its caller timeout expires.  An active runtime
// request with a bounce buffer detaches its caller: the worker frees that
// buffer only after the backend callback returns. A trusted direct request
// cannot detach its borrowed resident buffer, so its owner waits for physical
// completion after the timeout has been classified. WAIT_FOREVER remains an
// ownership-safe forever wait and keeps the backed-off reports.
const WATCHDOG_SLICE_TICKS: u64 = 5 * @as(u64, timer.DEFAULT_HZ);

/// Deadman probe (0.60.20): is any request slot executing in a driver
/// callback for longer than `threshold` ticks?  The runtime worker calls
/// the driver synchronously; a slot that stays .active that long means the
/// worker is stuck (or spinning) INSIDE the driver -- invisible to
/// blocked-task scans.  Racy read-only scan, crash-class trigger only.
pub fn hasStalledExecution(now: u64, threshold: u64) bool {
    return r4os_rust_block_stalled(now, threshold);
}
/// Framebuffer-direct variant of dumpStalledExecutions (0.60.20).
pub fn dumpStalledToDiag(now: u64, threshold: u64) void {
    r4os_rust_block_dump(now, threshold, true);
}
/// Paints every stalled execution (see hasStalledExecution) to the log.
pub fn dumpStalledExecutions(now: u64, threshold: u64) void {
    r4os_rust_block_dump(now, threshold, false);
}
fn putDec(value: u64) void {
    var digits: [20]u8 = undefined;
    if (value == 0) {
        k.puts("0");
        return;
    }
    var n = value;
    var i: usize = digits.len;
    while (n > 0) : (n /= 10) {
        i -= 1;
        digits[i] = '0' + @as(u8, @intCast(n % 10));
    }
    k.puts(digits[i..]);
}

extern fn r4os_rust_block_workers_init() callconv(.c) void;
extern fn r4os_rust_block_workers_start() callconv(.c) bool;
extern fn r4os_rust_block_workers_ready() callconv(.c) bool;
extern fn r4os_rust_block_workers_alive() callconv(.c) bool;
extern fn r4os_rust_block_workers_summary(*RuntimeSummary) callconv(.c) void;
extern fn r4os_rust_block_worker_alive(u8) callconv(.c) bool;
extern fn r4os_rust_block_worker_start(u8, bool) callconv(.c) bool;
extern fn r4os_rust_block_worker_lane(u8) callconv(.c) u8;
extern fn r4os_rust_block_worker_signal(u8) callconv(.c) void;

const NativeDeviceState = extern struct { state: *State, resetting: *bool, active: *u32, stats: *Stats };
extern fn r4os_rust_block_device_state(*const NativeDeviceState, u8, u8, u8, u16) callconv(.c) bool;
fn changeDeviceState(device: *Device, operation: u8, kind: RequestKind, err: Error, sectors: u16) bool {
    const input = NativeDeviceState{ .state = &device.state, .resetting = &device.resetting, .active = &device.active_executions, .stats = &device.stats };
    return r4os_rust_block_device_state(&input, operation, @intFromEnum(kind), @intFromEnum(err), sectors);
}
comptime {
    if (@sizeOf(NativeDeviceState) != 32) @compileError("Native device status C view differs");
}

const NativeIoView = extern struct { reference: access.DeviceRef, sector_size: u32, sector_count: u64, max_sectors: u16, queue_depth: u16, timeout: u64, writable: bool, has_writer: bool, has_submit: bool, has_flush: bool, stats: *Stats };
const NativeTransfer = extern struct { sectors_completed: u16, err: u8 };
const NativeWrite = extern struct { lba: u64, sectors: u16, data: [*]const u8, len: usize, attempted: bool, ok: bool };
const NativeEnqueue = extern struct { kind: RequestKind, lba: u64, sectors: u16, buffer: ?[*]u8, data: ?[*]const u8, len: usize, ownership: block_dispatch.BufferOwnership };
extern fn r4os_rust_block_read(usize, u64, u16, [*]u8, usize, bool) callconv(.c) bool;
extern fn r4os_rust_block_write(usize, u64, u16, [*]const u8, usize, bool, *NativeTransfer) callconv(.c) void;
extern fn r4os_rust_block_write_batch(usize, *const access.DeviceRef, *[direct_write_batch_limit]NativeWrite, usize, bool) callconv(.c) usize;
extern fn r4os_rust_block_flush(usize) callconv(.c) bool;
export fn r4os_zig_block_io_view(device: *Device, out: *NativeIoView) callconv(.c) void {
    out.* = .{ .reference = device.access_ref, .sector_size = device.sector_size, .sector_count = device.sector_count, .max_sectors = device.max_sectors_per_request, .queue_depth = device.queue_depth, .timeout = device.timeout_ticks, .writable = device.writable, .has_writer = device.write_fn != null, .has_submit = device.async_submit_fn != null, .has_flush = device.flush_fn != null, .stats = &device.stats };
}
comptime {
    if (@sizeOf(NativeIoView) != 64 or @sizeOf(NativeTransfer) != 4 or @sizeOf(NativeWrite) != 40 or @sizeOf(NativeEnqueue) != 56) @compileError("Native logical block I/O C layout differs");
}

const NativeExecutionView = extern struct {
    io: NativeIoView,
    state: NativeDeviceState,
    slots: *[MAX_REQUEST_QUEUE_DEPTH]RequestSlot,
    mutex: *sync.Mutex,
    slot_available: *sync.WaitQueue,
    completion_available: *sync.WaitQueue,
    name: RegistryText,
    driver: RegistryText,
    index: u8,
    lane: u8,
    has_cancel: bool,
    has_reset: bool,
    bus: u8,
    queue_depth: *u16,
};
extern fn r4os_rust_block_register(*const Registration, *const Device, *const fn (*Device, *const Device, *const access.DeviceRef, u8) callconv(.c) void, *DeviceSlot) callconv(.c) bool;
extern fn r4os_rust_block_unregister(usize) callconv(.c) bool;
extern fn r4os_rust_block_sync_init(*const NativeExecutionView) callconv(.c) void;
extern fn r4os_rust_block_retirement_prepare(usize, *UnregisterToken) callconv(.c) bool;
extern fn r4os_rust_block_retirement_commit(*UnregisterToken) callconv(.c) bool;
extern fn r4os_rust_block_retirement_cancel(*UnregisterToken) callconv(.c) bool;
extern fn r4os_rust_block_exclude(u8) callconv(.c) bool;
extern fn r4os_rust_block_device_operation(usize, u8, u8, u8, u8, u8) callconv(.c) u32;
extern fn r4os_rust_block_reset(usize, u32) callconv(.c) bool;
extern fn r4os_rust_block_complete(u64, i32, u32) callconv(.c) void;
extern fn r4os_rust_block_label(u8, u8, *RegistryText) callconv(.c) void;
extern fn r4os_rust_block_stalled(u64, u64) callconv(.c) bool;
extern fn r4os_rust_block_dump(u64, u64, bool) callconv(.c) void;
extern fn r4os_rust_block_nvme_probe() callconv(.c) bool;
export fn r4os_zig_block_execution_view(device: *Device, out: *NativeExecutionView) callconv(.c) void {
    r4os_zig_block_io_view(device, &out.io);
    out.state = .{ .state = &device.state, .resetting = &device.resetting, .active = &device.active_executions, .stats = &device.stats };
    out.slots = &device.request_slots;
    out.mutex = &device.queue_lock;
    out.slot_available = &device.slot_available;
    out.completion_available = &device.completion_available;
    out.name = registryText(device.name);
    out.driver = registryText(device.driver);
    out.index = device.slot_index;
    out.lane = device.controller_lane;
    out.has_cancel = device.async_cancel_fn != null;
    out.has_reset = device.async_reset_fn != null;
    out.bus = @intFromEnum(device.bus);
    out.queue_depth = &device.queue_depth;
}
export fn r4os_zig_block_backend(device: *Device, request: ?*const RequestExecution, operation: u8, reason: u32) callconv(.c) i32 {
    switch (operation) {
        1 => {
            const r = request.?;
            return @intFromBool(device.read_fn(device.ctx, r.lba, r.sectors, r.buffer.?[0..r.buffer_len]));
        },
        2 => {
            const r = request.?;
            return @intFromBool(device.write_fn.?(device.ctx, r.lba, r.sectors, r.const_buffer.?[0..r.buffer_len]));
        },
        3 => return @intFromBool(device.flush_fn.?(device.ctx)),
        4 => {
            const r = request.?;
            const public_request = AsyncRequest{ .handle = r.backend_handle, .kind = r.kind, .lba = r.lba, .sectors = r.sectors, .buffer = r.buffer, .const_buffer = r.const_buffer, .buffer_len = r.buffer_len, .complete = asyncBackendComplete };
            return device.async_submit_fn.?(device.ctx, &public_request);
        },
        5 => return device.async_cancel_fn.?(device.ctx, request.?.backend_handle, reason),
        6 => return device.async_reset_fn.?(device.ctx, reason),
        else => unreachable,
    }
}
export fn r4os_zig_block_mount_index(index: usize, out: *usize) callconv(.c) bool {
    const mounted = drive.atIndex(index) orelse return false;
    out.* = mounted.block_device_index orelse return false;
    return true;
}
export fn r4os_zig_block_incident_begin() callconv(.c) u64 {
    return diag_screen.beginResolvableIncident().generation;
}
export fn r4os_zig_block_incident_resolve(generation: u64) callconv(.c) void {
    _ = diag_screen.resolveIncident(.{ .generation = generation });
}
export fn r4os_zig_block_text(channel: u8, data: [*]const u8, len: usize) callconv(.c) void {
    if (channel == 1) diag_screen.write(data[0..len]) else k.puts(data[0..len]);
}
export fn r4os_zig_block_decimal(channel: u8, value: u64) callconv(.c) void {
    if (channel == 1) diag_screen.writeDec(value) else k.putDec(value);
}
export fn r4os_zig_block_line(channel: u8) callconv(.c) void {
    if (channel == 1) diag_screen.endLine() else k.puts("\r\n");
}
comptime {
    if (@sizeOf(NativeExecutionView) != 176 or @sizeOf(UnregisterToken) != 24 or sync.LockRank.block_device != 160) @compileError("Native block execution/sync/retirement C layout differs");
}

// Boot-option-gated runtime acceptance for controller ownership. A deliberately
// blocked callback on one synthetic controller must not hold up a request on
// another controller. Static buffers also exercise the borrowed-resident path
// and prove that this path performs neither a bounce allocation nor a copy.
var dispatch_test_slow_entered = sync.EventV2.initMode(false, .auto_reset);
var dispatch_test_slow_release = sync.EventV2.initMode(false, .auto_reset);
var dispatch_test_slow_done = sync.EventV2.initMode(false, .auto_reset);
var dispatch_test_fast_done = sync.EventV2.initMode(false, .auto_reset);
var dispatch_test_slow_index: usize = 0;
var dispatch_test_fast_index: usize = 0;
var dispatch_test_slow_ok = false;
var dispatch_test_fast_ok = false;
var dispatch_test_slow_buffer: [512]u8 = .{0} ** 512;
var dispatch_test_fast_buffer: [512]u8 = .{0} ** 512;

pub fn parallelDispatchSelfTest() bool {
    if (!runtimeWorkerReady()) return dispatchSelfTestResult(false, .{}, .{});
    dispatch_test_slow_entered = sync.EventV2.initMode(false, .auto_reset);
    dispatch_test_slow_release = sync.EventV2.initMode(false, .auto_reset);
    dispatch_test_slow_done = sync.EventV2.initMode(false, .auto_reset);
    dispatch_test_fast_done = sync.EventV2.initMode(false, .auto_reset);
    dispatch_test_slow_ok = false;
    dispatch_test_fast_ok = false;
    dispatch_test_slow_buffer = .{0} ** dispatch_test_slow_buffer.len;
    dispatch_test_fast_buffer = .{0} ** dispatch_test_fast_buffer.len;
    const before = runtimeWorkerSummary();

    const slow_index = register(.{
        .name = "dispatch-test-slow",
        .controller = "dispatch-test-controller-slow",
        .bus = .ram,
        .sector_size = 512,
        .sector_count = 1,
        .queue_depth = 1,
        .ctx = null,
        .read_fn = dispatchTestSlowRead,
    }) orelse return dispatchSelfTestResult(false, before, runtimeWorkerSummary());
    dispatch_test_slow_index = slow_index;
    const fast_index = register(.{
        .name = "dispatch-test-fast",
        .controller = "dispatch-test-controller-fast",
        .bus = .ram,
        .sector_size = 512,
        .sector_count = 1,
        .queue_depth = 1,
        .ctx = null,
        .read_fn = dispatchTestFastRead,
    }) orelse {
        _ = unregister(slow_index);
        return dispatchSelfTestResult(false, before, runtimeWorkerSummary());
    };
    dispatch_test_fast_index = fast_index;

    var ok = true;
    var slow_started = false;
    var fast_started = false;
    var fast_observed = false;
    if (sched_task.createKernelThreadWithRole("blk-test-slow", dispatchTestSlowRequester, .batch) == null) {
        ok = false;
    } else {
        slow_started = true;
    }
    if (slow_started and dispatch_test_slow_entered.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled) {
        ok = false;
    } else if (slow_started and sched_task.createKernelThreadWithRole("blk-test-fast", dispatchTestFastRequester, .batch) == null) {
        ok = false;
    } else if (slow_started) {
        fast_started = true;
    }
    if (fast_started and dispatch_test_fast_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) == .signaled) {
        fast_observed = true;
    } else if (fast_started) {
        ok = false;
    }

    dispatch_test_slow_release.signal();
    if (slow_started and dispatch_test_slow_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled) {
        ok = false;
    }
    if (fast_started and !fast_observed and
        dispatch_test_fast_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled)
    {
        ok = false;
    }
    const after = runtimeWorkerSummary();
    ok = dispatch_test_slow_ok and
        dispatch_test_fast_ok and
        after.controller_count >= before.controller_count + 2 and
        after.worker_count >= before.worker_count + 2 and
        after.worker_parallel_active_max >= 2 and
        after.direct_requests >= before.direct_requests + 2 and
        after.bounce_allocations == before.bounce_allocations and
        after.bounce_copy_bytes == before.bounce_copy_bytes and ok;

    const fast_unregistered = unregister(fast_index);
    const slow_unregistered = unregister(slow_index);
    ok = fast_unregistered and slow_unregistered and ok;
    const dispatch_ok = dispatchSelfTestResult(ok, before, after);
    const async_ok = asyncDispatchSelfTest();
    return dispatch_ok and async_ok;
}

fn dispatchTestSlowRead(_: ?*anyopaque, _: u64, _: u16, out: []u8) bool {
    dispatch_test_slow_entered.signal();
    if (dispatch_test_slow_release.waitResult(sync.WAIT_FOREVER) != .signaled) return false;
    @memset(out, 0x53);
    return true;
}

fn dispatchTestFastRead(_: ?*anyopaque, _: u64, _: u16, out: []u8) bool {
    @memset(out, 0x46);
    return true;
}

fn dispatchTestSlowRequester() callconv(.c) void {
    dispatch_test_slow_ok = readDirect(dispatch_test_slow_index, 0, 1, dispatch_test_slow_buffer[0..]);
    dispatch_test_slow_done.signal();
    scheduler.exitCurrent();
}

fn dispatchTestFastRequester() callconv(.c) void {
    dispatch_test_fast_ok = readDirect(dispatch_test_fast_index, 0, 1, dispatch_test_fast_buffer[0..]);
    dispatch_test_fast_done.signal();
    scheduler.exitCurrent();
}

fn dispatchSelfTestResult(ok: bool, before: RuntimeSummary, after: RuntimeSummary) bool {
    k.puts("BLOCKDISPATCHCHECK ");
    k.puts(if (ok) "OK" else "FAIL");
    k.puts(" controllers=");
    k.putDec(after.controller_count);
    k.puts(" workers=");
    k.putDec(after.worker_count);
    k.puts(" parallelMax=");
    k.putDec(after.worker_parallel_active_max);
    k.puts(" directDelta=");
    k.putDec(after.direct_requests -| before.direct_requests);
    k.puts(" bounceAllocDelta=");
    k.putDec(after.bounce_allocations -| before.bounce_allocations);
    k.puts(" bounceCopyDelta=");
    k.putDec(after.bounce_copy_bytes -| before.bounce_copy_bytes);
    k.puts("\r\n");
    return ok;
}

const AsyncTestState = struct {
    submissions: u32 = 0,
    cancels: u32 = 0,
    timeout_cancels: u32 = 0,
    reset_cancels: u32 = 0,
    resets: u32 = 0,
    requests: [12]AsyncRequest = undefined,
};

var async_test_batch_done = sync.EventV2.initMode(false, .auto_reset);
var async_test_batch_ok = false;
var async_test_state: AsyncTestState = .{};
var async_test_submitted = sync.EventV2.initMode(false, .auto_reset);
var async_test_first_done = sync.EventV2.initMode(false, .auto_reset);
var async_test_second_done = sync.EventV2.initMode(false, .auto_reset);
var async_test_reset_done = sync.EventV2.initMode(false, .auto_reset);
var async_test_timeout_done = sync.EventV2.initMode(false, .auto_reset);
var async_test_flush_first_done = sync.EventV2.initMode(false, .auto_reset);
var async_test_flush_done = sync.EventV2.initMode(false, .auto_reset);
var async_test_flush_last_done = sync.EventV2.initMode(false, .auto_reset);
var async_test_index: usize = 0;
var async_test_first_ok = false;
var async_test_second_ok = false;
var async_test_reset_ok = true;
var async_test_timeout_ok = true;
var async_test_flush_first_ok = false;
var async_test_flush_ok = false;
var async_test_flush_last_ok = false;
var async_test_first_buffer: [512]u8 = .{0} ** 512;
var async_test_second_buffer: [512]u8 = .{0} ** 512;
var async_test_reset_buffer: [512]u8 = .{0} ** 512;
var async_test_timeout_buffer: [512]u8 = .{0} ** 512;
var async_test_flush_first_buffer: [512]u8 = .{0} ** 512;
var async_test_flush_last_buffer: [512]u8 = .{0} ** 512;

fn asyncDispatchSelfTest() bool {
    async_test_state = .{};
    async_test_batch_ok = false;
    async_test_batch_done = sync.EventV2.initMode(false, .auto_reset);
    async_test_submitted = sync.EventV2.initMode(false, .auto_reset);
    async_test_first_done = sync.EventV2.initMode(false, .auto_reset);
    async_test_second_done = sync.EventV2.initMode(false, .auto_reset);
    async_test_reset_done = sync.EventV2.initMode(false, .auto_reset);
    async_test_timeout_done = sync.EventV2.initMode(false, .auto_reset);
    async_test_flush_first_done = sync.EventV2.initMode(false, .auto_reset);
    async_test_flush_done = sync.EventV2.initMode(false, .auto_reset);
    async_test_flush_last_done = sync.EventV2.initMode(false, .auto_reset);
    async_test_first_ok = false;
    async_test_second_ok = false;
    async_test_reset_ok = true;
    async_test_timeout_ok = true;
    async_test_flush_first_ok = false;
    async_test_flush_ok = false;
    async_test_flush_last_ok = false;
    async_test_first_buffer = .{0} ** async_test_first_buffer.len;
    async_test_second_buffer = .{0} ** async_test_second_buffer.len;
    async_test_reset_buffer = .{0} ** async_test_reset_buffer.len;
    async_test_timeout_buffer = .{0} ** async_test_timeout_buffer.len;
    async_test_flush_first_buffer = .{0} ** async_test_flush_first_buffer.len;
    async_test_flush_last_buffer = .{0} ** async_test_flush_last_buffer.len;
    const before = runtimeWorkerSummary();

    const index = register(.{
        .name = "dispatch-test-async",
        .controller = "dispatch-test-controller-async",
        .bus = .ram,
        .sector_size = 512,
        .sector_count = 8,
        .writable = true,
        .queue_depth = 2,
        .timeout_ticks = 50,
        .ctx = &async_test_state,
        .read_fn = asyncTestSyncRead,
        .async_submit_fn = asyncTestSubmit,
        .async_cancel_fn = asyncTestCancel,
        .async_reset_fn = asyncTestReset,
    }) orelse return asyncDispatchSelfTestResult(false, before, runtimeWorkerSummary());
    async_test_index = index;

    var ok = true;
    if (sched_task.createKernelThreadWithRole("blk-async-a", asyncTestFirstRequester, .batch) == null or
        sched_task.createKernelThreadWithRole("blk-async-b", asyncTestSecondRequester, .batch) == null)
    {
        ok = false;
    }
    if (ok and !asyncTestWaitForSubmissions(2)) ok = false;
    if (ok) {
        if (get(index)) |device| {
            if (device.active_executions != 2) ok = false;
        } else {
            ok = false;
        }
    }
    if (ok) {
        const second = asyncTestFindRequest(.read, 1) orelse return asyncDispatchSelfTestResult(false, before, runtimeWorkerSummary());
        const first = asyncTestFindRequest(.read, 0) orelse return asyncDispatchSelfTestResult(false, before, runtimeWorkerSummary());
        if (second.buffer) |buffer| @memset(buffer[0..second.buffer_len], 0xB2);
        if (first.buffer) |buffer| @memset(buffer[0..first.buffer_len], 0xA1);
        second.complete(second.handle, ASYNC_RESULT_OK, @intCast(second.buffer_len));
        second.complete(second.handle, ASYNC_RESULT_OK, @intCast(second.buffer_len));
        first.complete(first.handle, ASYNC_RESULT_OK, @intCast(first.buffer_len));
    }
    if (async_test_first_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled or
        async_test_second_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled)
    {
        ok = false;
    }
    ok = async_test_first_ok and async_test_second_ok and
        async_test_first_buffer[0] == 0xA1 and async_test_second_buffer[0] == 0xB2 and ok;

    const reset_task = sched_task.createKernelThreadWithRole("blk-async-reset", asyncTestResetRequester, .batch);
    if (reset_task == null) {
        ok = false;
    }
    if (ok and !asyncTestWaitForSubmissions(3)) ok = false;
    var late_handle: u64 = 0;
    if (ok) {
        late_handle = (asyncTestFindRequest(.read, 2) orelse return asyncDispatchSelfTestResult(false, before, runtimeWorkerSummary())).handle;
        const kill_deferrals_before = sched_task.killHeldLockDeferrals();
        if (sched_task.kill(native_tasks.get(reset_task.?, .id)) or sched_task.killHeldLockDeferrals() <= kill_deferrals_before) ok = false;
        // Unload admission must remain transactional while either a caller pin
        // or a hardware-owned request is live.
        if (unregister(index)) ok = false;
        if (!reset(index, CANCEL_REASON_RESET)) ok = false;
    }
    if (async_test_reset_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled) ok = false;
    if (late_handle != 0) asyncBackendComplete(late_handle, ASYNC_RESULT_OK, 512);

    if (sched_task.createKernelThreadWithRole("blk-async-timeout", asyncTestTimeoutRequester, .batch) == null) {
        ok = false;
    }
    if (ok and !asyncTestWaitForSubmissions(4)) ok = false;
    if (async_test_timeout_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled) ok = false;
    ok = !async_test_timeout_ok and async_test_state.timeout_cancels == 1 and ok;

    // Queue read -> flush -> read while depth two is available. The flush may
    // not be submitted beside the first read, and the later read may not pass
    // it. This covers the actual async path rather than only the pure policy.
    if (sched_task.createKernelThreadWithRole("blk-async-before-flush", asyncTestFlushFirstRequester, .batch) == null) {
        ok = false;
    }
    if (ok and !asyncTestWaitForSubmissions(5)) ok = false;
    if (sched_task.createKernelThreadWithRole("blk-async-flush", asyncTestFlushRequester, .batch) == null) {
        ok = false;
    }
    if (ok and (async_test_submitted.waitResult(5) != .timeout or queueUsed(index) != 2)) ok = false;
    if (sched_task.createKernelThreadWithRole("blk-async-after-flush", asyncTestFlushLastRequester, .batch) == null) {
        ok = false;
    }
    if (ok and (async_test_submitted.waitResult(5) != .timeout or queueUsed(index) != 3)) ok = false;
    if (ok) {
        const first = async_test_state.requests[4];
        if (first.kind != .read or first.lba != 4) {
            ok = false;
        } else {
            if (first.buffer) |buffer| @memset(buffer[0..first.buffer_len], 0xC4);
            first.complete(first.handle, ASYNC_RESULT_OK, @intCast(first.buffer_len));
        }
    }
    if (ok and !asyncTestWaitForSubmissions(6)) ok = false;
    if (ok) {
        const flush_request = async_test_state.requests[5];
        if (flush_request.kind != .flush or async_test_state.submissions != 6) {
            ok = false;
        } else {
            flush_request.complete(flush_request.handle, ASYNC_RESULT_OK, 0);
        }
    }
    if (ok and !asyncTestWaitForSubmissions(7)) ok = false;
    if (ok) {
        const last = async_test_state.requests[6];
        if (last.kind != .read or last.lba != 5) {
            ok = false;
        } else {
            if (last.buffer) |buffer| @memset(buffer[0..last.buffer_len], 0xD5);
            last.complete(last.handle, ASYNC_RESULT_OK, @intCast(last.buffer_len));
        }
    }
    if (async_test_flush_first_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled or
        async_test_flush_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled or
        async_test_flush_last_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled)
    {
        ok = false;
    }
    ok = async_test_flush_first_ok and async_test_flush_ok and async_test_flush_last_ok and
        async_test_flush_first_buffer[0] == 0xC4 and async_test_flush_last_buffer[0] == 0xD5 and ok;

    // A single batch caller must fill both hardware slots, retain its
    // registration across out-of-order partial failure, and drain both.
    if (sched_task.createKernelThreadWithRole("blk-write-batch", asyncTestBatchRequester, .batch) == null) ok = false;
    if (ok and !asyncTestWaitForSubmissions(9)) ok = false;
    if (ok) {
        const first = async_test_state.requests[7];
        const second = async_test_state.requests[8];
        if (first.kind != .write or second.kind != .write or
            first.const_buffer == null or second.const_buffer == null or
            first.const_buffer.?[0] != 0xE1 or second.const_buffer.?[0] != 0xE2 or
            (get(index) orelse return false).active_executions != 2 or unregister(index)) ok = false;
        second.complete(second.handle, ASYNC_RESULT_ERROR, 0);
        first.complete(first.handle, ASYNC_RESULT_OK, @intCast(first.buffer_len));
    }
    if (async_test_batch_done.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled) ok = false;
    ok = async_test_batch_ok and ok;
    k.puts(if (async_test_batch_ok) "[BLOCKWRITEBATCH] result=OK pending=2 partial_failure=1 drained=1\r\n" else "[BLOCKWRITEBATCH] result=FAIL\r\n");

    const after = runtimeWorkerSummary();
    ok = !async_test_reset_ok and
        async_test_state.cancels == 2 and
        async_test_state.reset_cancels == 1 and
        async_test_state.resets == 1 and
        after.in_flight_high_water >= 2 and
        after.duplicate_completions > before.duplicate_completions and
        after.late_completions > before.late_completions and ok;
    ok = unregister(index) and ok;
    return asyncDispatchSelfTestResult(ok, before, after);
}

fn asyncTestBatchRequester() callconv(.c) void {
    defer async_test_batch_done.signal();
    @memset(&async_test_first_buffer, 0xE1);
    @memset(&async_test_second_buffer, 0xE2);
    const limits = directWriteLimits(async_test_index) orelse return;
    var writes = [_]DirectWrite{
        .{ .lba = 6, .sectors = 1, .data = &async_test_first_buffer },
        .{ .lba = 7, .sectors = 1, .data = &async_test_second_buffer },
    };
    const outcome = writeBatchDirect(async_test_index, limits.reference, &writes, true);
    async_test_batch_ok = outcome.pending_max == 2 and writes[0].attempted and writes[1].attempted and
        writes[0].ok and !writes[1].ok and queueUsed(async_test_index) == 0;
}

fn asyncTestWaitForSubmissions(expected: u32) bool {
    while (async_test_state.submissions < expected) {
        if (async_test_submitted.waitResult(2 * @as(u64, timer.DEFAULT_HZ)) != .signaled) return false;
    }
    return async_test_state.submissions == expected;
}

fn asyncTestFindRequest(kind: RequestKind, lba: u64) ?AsyncRequest {
    var index: usize = 0;
    while (index < async_test_state.submissions) : (index += 1) {
        const request = async_test_state.requests[index];
        if (request.kind == kind and request.lba == lba) return request;
    }
    return null;
}

fn asyncTestSyncRead(_: ?*anyopaque, _: u64, _: u16, _: []u8) bool {
    return false;
}

fn asyncTestSubmit(ctx: ?*anyopaque, request: *const AsyncRequest) i32 {
    const state: *AsyncTestState = @ptrCast(@alignCast(ctx orelse return -1));
    if (state.submissions >= state.requests.len) return -1;
    state.requests[state.submissions] = request.*;
    state.submissions += 1;
    async_test_submitted.signal();
    return 0;
}

fn asyncTestCancel(ctx: ?*anyopaque, handle: u64, reason: u32) i32 {
    const state: *AsyncTestState = @ptrCast(@alignCast(ctx orelse return -1));
    state.cancels += 1;
    if (reason == CANCEL_REASON_TIMEOUT) {
        state.timeout_cancels += 1;
        var index: usize = 0;
        while (index < state.submissions) : (index += 1) {
            const request = state.requests[index];
            if (request.handle != handle) continue;
            request.complete(handle, ASYNC_RESULT_CANCELLED, 0);
            return 0;
        }
        return -1;
    }
    if (reason == CANCEL_REASON_RESET) state.reset_cancels += 1;
    return 0;
}

fn asyncTestReset(ctx: ?*anyopaque, _: u32) i32 {
    const state: *AsyncTestState = @ptrCast(@alignCast(ctx orelse return -1));
    state.resets += 1;
    return 0;
}

fn asyncTestFirstRequester() callconv(.c) void {
    async_test_first_ok = readDirect(async_test_index, 0, 1, async_test_first_buffer[0..]);
    async_test_first_done.signal();
    scheduler.exitCurrent();
}

fn asyncTestSecondRequester() callconv(.c) void {
    async_test_second_ok = readDirect(async_test_index, 1, 1, async_test_second_buffer[0..]);
    async_test_second_done.signal();
    scheduler.exitCurrent();
}

fn asyncTestResetRequester() callconv(.c) void {
    async_test_reset_ok = readDirect(async_test_index, 2, 1, async_test_reset_buffer[0..]);
    async_test_reset_done.signal();
    scheduler.exitCurrent();
}

fn asyncTestTimeoutRequester() callconv(.c) void {
    async_test_timeout_ok = readDirect(async_test_index, 3, 1, async_test_timeout_buffer[0..]);
    async_test_timeout_done.signal();
    scheduler.exitCurrent();
}

fn asyncTestFlushFirstRequester() callconv(.c) void {
    async_test_flush_first_ok = readDirect(async_test_index, 4, 1, async_test_flush_first_buffer[0..]);
    async_test_flush_first_done.signal();
    scheduler.exitCurrent();
}

fn asyncTestFlushRequester() callconv(.c) void {
    async_test_flush_ok = flush(async_test_index);
    async_test_flush_done.signal();
    scheduler.exitCurrent();
}

fn asyncTestFlushLastRequester() callconv(.c) void {
    async_test_flush_last_ok = readDirect(async_test_index, 5, 1, async_test_flush_last_buffer[0..]);
    async_test_flush_last_done.signal();
    scheduler.exitCurrent();
}

fn asyncDispatchSelfTestResult(ok: bool, before: RuntimeSummary, after: RuntimeSummary) bool {
    k.puts("BLOCKASYNCCHECK ");
    k.puts(if (ok) "OK" else "FAIL");
    k.puts(" inflightMax=");
    k.putDec(after.in_flight_high_water);
    k.puts(" submits=");
    k.putDec(after.async_submissions -| before.async_submissions);
    k.puts(" completions=");
    k.putDec(after.async_completions -| before.async_completions);
    k.puts(" duplicates=");
    k.putDec(after.duplicate_completions -| before.duplicate_completions);
    k.puts(" late=");
    k.putDec(after.late_completions -| before.late_completions);
    k.puts(" resets=");
    k.putDec(after.async_resets -| before.async_resets);
    k.puts(" cancels=");
    k.putDec(after.async_cancel_requests -| before.async_cancel_requests);
    k.puts("\r\n");
    return ok;
}

fn busName(bus: Bus) []const u8 {
    return switch (bus) {
        .unknown => "unknown",
        .ata => "ata",
        .ahci => "ahci",
        .nvme => "nvme",
        .usb => "usb",
        .ram => "ram",
        .virtio => "virtio",
    };
}

fn stateName(state: State) []const u8 {
    return switch (state) {
        .registered => "registered",
        .active => "active",
        .busy => "busy",
        .recovering => "recovering",
        .failed => "failed",
    };
}

fn errorName(err: Error) []const u8 {
    return switch (err) {
        .none => "none",
        .busy => "busy",
        .invalid_request => "invalid-request",
        .request_too_large => "request-too-large",
        .out_of_range => "out-of-range",
        .buffer_too_small => "buffer-too-small",
        .no_writer => "no-writer",
        .timeout => "timeout",
        .backend_read => "backend-read",
        .backend_write => "backend-write",
        .backend_flush => "backend-flush",
        .cancelled => "cancelled",
        .reset => "reset",
        .shutdown => "shutdown",
    };
}

fn sourceName(source: Source) []const u8 {
    return switch (source) {
        .builtin => "built-in",
        .preload => "preload",
        .disk => "disk",
    };
}

fn kindName(kind: RequestKind) []const u8 {
    return switch (kind) {
        .none => "none",
        .read => "read",
        .write => "write",
        .flush => "flush",
    };
}

fn strEqIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        const ca = if (a[i] >= 'A' and a[i] <= 'Z') a[i] + 32 else a[i];
        const cb = if (b[i] >= 'A' and b[i] <= 'Z') b[i] + 32 else b[i];
        if (ca != cb) return false;
    }
    return true;
}

test "preload v2 storage uses synchronous boot fallback before async runtime" {
    try std.testing.expect(!useAsyncSubmission(.boot_inline, true));
    try std.testing.expect(useAsyncSubmission(.runtime_worker, true));
    try std.testing.expect(!useAsyncSubmission(.runtime_worker, false));
}
