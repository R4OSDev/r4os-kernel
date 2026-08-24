const diag_screen = @import("../kernel/diag_screen.zig");
const block_split = @import("block_split.zig");
const drive = @import("../fs/drive.zig");
const heap = @import("../memory/heap.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const k = @import("../kernel/log.zig");
const std = @import("std");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const sched_task = @import("../sched/task.zig");
const task_context = @import("../sched/task_context.zig");
const timer = @import("../kernel/timer.zig");

const MAX_DEVICES: usize = 8;
pub const MAX_REQUEST_QUEUE_DEPTH: usize = 16;

pub const ReadFn = *const fn (ctx: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool;
pub const WriteFn = *const fn (ctx: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool;
pub const FlushFn = *const fn (ctx: ?*anyopaque) bool;

pub const Bus = enum {
    unknown,
    ata,
    ahci,
    nvme,
    usb,
    ram,
    virtio,
};

pub const State = enum {
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

pub const RequestKind = enum {
    none,
    read,
    write,
    flush,
};

pub const RequestSnapshot = struct {
    id: u64 = 0,
    kind: RequestKind = .none,
    lba: u64 = 0,
    sectors: u16 = 0,
};

pub const SenseSnapshot = struct {
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

const RequestSlot = struct {
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
    // returned.  A finite timeout detaches the caller and transfers bounce-
    // buffer ownership to that late completion; the active slot itself is
    // never recycled early.
    timeout_requested: bool = false,
    caller_detached: bool = false,
    backend_owns_buffer: bool = false,
};

const RequestExecution = struct {
    slot_index: usize,
    id: u64,
    kind: RequestKind,
    lba: u64,
    sectors: u16,
    buffer: ?[*]u8,
    const_buffer: ?[*]const u8,
    buffer_len: usize,
    start_tick: u64,
    mode: ExecutionMode,
};

const RequestResult = struct {
    ok: bool = false,
    err: Error = .none,
    buffer_detached: bool = false,
};

const ExecutionMode = enum(u8) {
    boot_inline,
    runtime_worker,
};

pub const Error = enum {
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
};

/// Exact committed prefix of a logical write.  Filesystem/cache callers use
/// this to invalidate only sectors that definitely reached the backend when
/// a later backend-sized chunk fails.
pub const TransferResult = struct {
    sectors_completed: u16 = 0,
    err: Error = .none,
};

pub const Stats = struct {
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
    last_sense: SenseSnapshot = .{},
    last_error: Error = .none,
};

pub const RuntimeSummary = struct {
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
};

pub const Device = struct {
    name: []const u8,
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
    state: State = .registered,
    stats: Stats = .{},
    active_executions: u32 = 0,
    request_slots: [MAX_REQUEST_QUEUE_DEPTH]RequestSlot = .{RequestSlot{}} ** MAX_REQUEST_QUEUE_DEPTH,
    slot_available: sync.WaitQueue = sync.WaitQueue.init(),
    completion_available: sync.WaitQueue = sync.WaitQueue.init(),
    queue_lock: sync.Mutex = sync.Mutex.initClass("block-device", sync.LockRank.block_device, .sleepable),
};

const DeviceSlot = struct {
    used: bool = false,
    retiring: bool = false,
    pin_count: u32 = 0,
    retire_generation: u64 = 0,
    device: Device = undefined,
};

const DevicePin = struct {
    slot: *DeviceSlot,
    device: *Device,
    unwind: task_context.UnwindToken,
    active: bool = true,
};

// A prepared unregister owns the retiring admission barrier until it is
// either committed or cancelled. Index plus retirement generation avoids
// exposing DeviceSlot and rejects copied stale tokens after cancel/reuse.
pub const UnregisterToken = struct {
    index: usize = 0,
    generation: u64 = 0,
    active: bool = false,
};

// Device addresses are stable for the complete lifetime of one registration.
// Unregister leaves a tombstone instead of shifting later Device values (and
// their embedded WaitQueues/Mutex); register may reuse only a quiescent slot.
var devices: [MAX_DEVICES]DeviceSlot = .{DeviceSlot{}} ** MAX_DEVICES;
var device_count: usize = 0;
var device_slot_count: usize = 0;
var runtime_worker_started = false;
var runtime_worker_task_id: u32 = 0;
var runtime_worker_task_generation: u64 = 0;
var runtime_event = sync.EventV2.initMode(false, .auto_reset);
var runtime_summary: RuntimeSummary = .{};

pub fn init() void {
    devices = .{DeviceSlot{}} ** MAX_DEVICES;
    device_count = 0;
    device_slot_count = 0;
    runtime_worker_started = false;
    runtime_worker_task_id = 0;
    runtime_worker_task_generation = 0;
    runtime_event = sync.EventV2.initMode(false, .auto_reset);
    runtime_summary = .{};
}

pub fn initRuntimeWorker() bool {
    if (runtimeWorkerIdentityAlive()) return true;
    runtime_worker_started = false;
    runtime_worker_task_id = 0;
    runtime_worker_task_generation = 0;
    const worker = sched_task.createKernelThreadCriticalWithRole("block-work", workerMain, .batch) orelse {
        runtime_summary.worker_started = 0;
        runtime_summary.worker_task_id = 0;
        return false;
    };
    runtime_worker_started = true;
    runtime_worker_task_id = worker.id;
    runtime_worker_task_generation = worker.generation;
    runtime_summary.worker_started = 1;
    runtime_summary.worker_task_id = worker.id;
    return true;
}

pub fn runtimeWorkerReady() bool {
    return scheduler.currentId() != null and runtimeWorkerIdentityAlive();
}

pub fn runtimeWorkerSummary() RuntimeSummary {
    var out = runtime_summary;
    const alive = runtimeWorkerIdentityAlive();
    out.worker_started = if (alive) 1 else 0;
    out.worker_task_id = if (alive) runtime_worker_task_id else 0;
    return out;
}

fn runtimeWorkerIdentityAlive() bool {
    return runtime_worker_started and
        runtime_worker_task_id != 0 and
        runtime_worker_task_generation != 0 and
        sched_task.isAliveIdentity(runtime_worker_task_id, runtime_worker_task_generation);
}

pub fn register(device: Device) ?usize {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (device_count >= MAX_DEVICES) return null;
    if (findByNameLocked(device.name) != null) return null;
    if (device.queue_depth == 0 or @as(usize, device.queue_depth) > MAX_REQUEST_QUEUE_DEPTH) return null;

    var target_index: usize = 0;
    while (target_index < device_slot_count and devices[target_index].used) : (target_index += 1) {}
    if (target_index == device_slot_count) {
        if (device_slot_count >= devices.len) return null;
        device_slot_count += 1;
    }

    var normalized = device;
    normalized.state = .registered;
    normalized.stats = .{};
    normalized.active_executions = 0;
    normalized.request_slots = .{RequestSlot{}} ** MAX_REQUEST_QUEUE_DEPTH;
    normalized.slot_available = sync.WaitQueue.init();
    normalized.completion_available = sync.WaitQueue.init();
    normalized.queue_lock = sync.Mutex.initClass("block-device", sync.LockRank.block_device, .sleepable);
    devices[target_index].device = normalized;
    devices[target_index].retiring = false;
    devices[target_index].pin_count = 0;
    devices[target_index].used = true;
    device_count += 1;
    return target_index;
}

pub fn unregister(index: usize) bool {
    var token = prepareUnregister(index) orelse return false;
    if (commitUnregister(&token)) return true;
    _ = cancelUnregister(&token);
    return false;
}

// Stop new admissions and prove the complete device lifetime is quiescent,
// but leave the registration intact. Callers may prepare several devices and
// cancel all of them without having partially removed an owner.
pub fn prepareUnregister(index: usize) ?UnregisterToken {
    const slot = beginRetirement(index) orelse return null;
    if (hasMountedDrive(index)) {
        cancelRetirement(slot);
        return null;
    }

    const device = &slot.device;
    if (!deviceIsQuiescent(device)) {
        cancelRetirement(slot);
        return null;
    }

    // Closing is the final successful-unregister boundary. It prevents the
    // stable embedded synchronization objects from accepting a late waiter
    // while the slot is a tombstone. A surprising raced waiter keeps the
    // registration alive and all queues are reopened after cancellation.
    const slot_waiters = device.slot_available.close(.cancelled);
    const completion_waiters = device.completion_available.close(.cancelled);
    const mutex_waiters = device.queue_lock.queue.close(.cancelled);
    if (slot_waiters != 0 or completion_waiters != 0 or mutex_waiters != 0) {
        _ = device.slot_available.reopen();
        _ = device.completion_available.reopen();
        _ = device.queue_lock.queue.reopen();
        cancelRetirement(slot);
        return null;
    }

    return .{
        .index = index,
        .generation = slot.retire_generation,
        .active = true,
    };
}

pub fn commitUnregister(token: *UnregisterToken) bool {
    if (!token.active) return false;
    const slot = preparedSlot(token.index, token.generation) orelse return false;
    if (!commitRetirement(slot)) return false;
    token.active = false;
    return true;
}

pub fn cancelUnregister(token: *UnregisterToken) bool {
    if (!token.active) return false;
    const slot = preparedSlot(token.index, token.generation) orelse {
        token.active = false;
        return false;
    };
    const device = &slot.device;

    _ = device.slot_available.reopen();
    _ = device.completion_available.reopen();
    _ = device.queue_lock.queue.reopen();
    cancelRetirement(slot);
    token.active = false;
    return true;
}

fn preparedSlot(index: usize, generation: u64) ?*DeviceSlot {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (index >= device_slot_count) return null;
    const slot = &devices[index];
    if (!slot.used or !slot.retiring or slot.retire_generation != generation) return null;
    return slot;
}

fn beginRetirement(index: usize) ?*DeviceSlot {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (index >= device_slot_count) return null;
    const slot = &devices[index];
    if (!slot.used or slot.retiring) return null;
    if (slot.retire_generation == 0xFFFF_FFFF_FFFF_FFFF) return null;

    // Admission is stopped before the pin snapshot. A live operation makes
    // unregister a non-destructive retryable failure; it can then unwind its
    // task-owned pin without ever observing reused Device storage.
    slot.retiring = true;
    slot.retire_generation += 1;
    if (slot.pin_count != 0) {
        slot.retiring = false;
        return null;
    }
    return slot;
}

fn cancelRetirement(slot: *DeviceSlot) void {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (slot.used) slot.retiring = false;
}

fn commitRetirement(slot: *DeviceSlot) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (!slot.used or !slot.retiring or slot.pin_count != 0) return false;
    slot.used = false;
    slot.retiring = false;
    if (device_count != 0) device_count -= 1;
    return true;
}

fn deviceIsQuiescent(device: *Device) bool {
    if (device.active_executions != 0 or countUsedSlots(device) != 0) return false;
    if (device.queue_lock.owner != 0 or device.queue_lock.depth != 0) return false;
    if (device.slot_available.hasWaiters()) return false;
    if (device.completion_available.hasWaiters()) return false;
    if (device.queue_lock.queue.hasWaiters()) return false;
    return true;
}

fn hasMountedDrive(block_index: usize) bool {
    var letter: u8 = 'A';
    while (letter <= 'Z') : (letter += 1) {
        const d = drive.get(letter) orelse continue;
        if (d.block_device_index != null and d.block_device_index.? == block_index) return true;
    }
    return false;
}

pub fn findByName(name: []const u8) ?usize {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    return findByNameLocked(name);
}

fn findByNameLocked(name: []const u8) ?usize {
    var index: usize = 0;
    while (index < device_slot_count) : (index += 1) {
        if (!devices[index].used) continue;
        if (strEqIgnoreCase(devices[index].device.name, name)) return index;
    }
    return null;
}

pub fn count() usize {
    return device_count;
}

pub fn slotCount() usize {
    return device_slot_count;
}

pub fn maxDevices() usize {
    return MAX_DEVICES;
}

pub fn get(index: usize) ?*const Device {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (index >= device_slot_count or !devices[index].used or devices[index].retiring) return null;
    return &devices[index].device;
}

fn pinDevice(index: usize) ?DevicePin {
    const unwind = task_context.enterUnwind();
    if (!unwind.admitted()) return null;

    const irq_flags = interrupts.saveAndDisable();
    if (index >= device_slot_count or
        !devices[index].used or
        devices[index].retiring or
        devices[index].pin_count == 0xFFFF_FFFF)
    {
        interrupts.restore(irq_flags);
        _ = task_context.leaveUnwind(unwind);
        return null;
    }
    const slot = &devices[index];
    slot.pin_count += 1;
    const device = &slot.device;
    interrupts.restore(irq_flags);
    return .{ .slot = slot, .device = device, .unwind = unwind };
}

fn unpinDevice(pin: *DevicePin) void {
    if (!pin.active) return;
    const irq_flags = interrupts.saveAndDisable();
    if (pin.slot.pin_count != 0) pin.slot.pin_count -= 1;
    pin.active = false;
    interrupts.restore(irq_flags);
    _ = task_context.leaveUnwind(pin.unwind);
}

pub fn queueUsed(index: usize) u32 {
    var pin = pinDevice(index) orelse return 0;
    defer unpinDevice(&pin);
    return countUsedSlots(pin.device);
}

pub fn beginBackendRecovery(index: usize) void {
    var pin = pinDevice(index) orelse return;
    defer unpinDevice(&pin);
    // Recovery runs inside the active backend callback, hence the normal
    // state is necessarily .busy here.  Preserve that distinction for the
    // watchdog instead of suppressing the only useful recovery telemetry.
    pin.device.state = .recovering;
}

pub fn finishBackendRecovery(index: usize, ok: bool) void {
    var pin = pinDevice(index) orelse return;
    defer unpinDevice(&pin);
    const device = pin.device;
    device.stats.backend_recoveries += 1;
    if (ok) {
        device.state = if (device.active_executions != 0) .busy else .active;
    } else {
        device.stats.backend_recovery_failures += 1;
        device.state = .failed;
    }
}

pub fn recordSense(index: usize, opcode: u8, key: u8, asc: u8, ascq: u8) void {
    var pin = pinDevice(index) orelse return;
    defer unpinDevice(&pin);
    const device = pin.device;
    device.stats.last_sense = .{
        .valid = true,
        .opcode = opcode,
        .key = key,
        .asc = asc,
        .ascq = ascq,
    };
}

pub fn busLabel(bus: Bus) []const u8 {
    return busName(bus);
}

pub fn stateLabel(state: State) []const u8 {
    return stateName(state);
}

pub fn errorLabel(err: Error) []const u8 {
    return errorName(err);
}

pub fn sourceLabel(source: Source) []const u8 {
    return sourceName(source);
}

pub fn read(index: usize, lba: u64, sectors: u16, out: []u8) bool {
    var pin = pinDevice(index) orelse return false;
    defer unpinDevice(&pin);
    const device = pin.device;
    if (validateRequest(device, lba, sectors, out.len)) |err| {
        recordReadFailure(device, err);
        return false;
    }
    var iterator = block_split.Iterator.init(sectors, device.max_sectors_per_request, device.sector_size) orelse {
        recordReadFailure(device, .invalid_request);
        return false;
    };
    while (iterator.next()) |chunk| {
        if (readChunk(
            device,
            lba + @as(u64, chunk.sector_offset),
            chunk.sectors,
            out[chunk.byte_offset .. chunk.byte_offset + chunk.byte_count],
        ) != .none) return false;
    }
    return true;
}

fn readChunk(device: *Device, lba: u64, sectors: u16, out: []u8) Error {
    const byte_count = @as(usize, sectors) * @as(usize, device.sector_size);

    // The runtime block worker is a different task.  It must never retain a
    // raw pointer into the caller's pageable R4X address space: a page-out
    // between enqueue and execution would make the storage worker fault back
    // into the storage path it is meant to service.  Kernel-heap bounce
    // memory is non-pageable and remains owned by this synchronous call until
    // the exact request has completed.  Early boot still executes inline and
    // therefore needs no allocation.
    const use_runtime_worker = runtimeWorkerReady();
    const bounce = if (use_runtime_worker)
        heap.alloc(byte_count, 16) orelse {
            recordReadFailure(device, .busy);
            return .busy;
        }
    else
        null;
    var release_bounce = true;
    defer if (release_bounce) if (bounce) |memory| {
        _ = heap.free(memory);
    };
    const request_buffer = if (bounce) |memory| memory.ptr else out.ptr;
    const request_id = enqueueRequest(device, .read, lba, sectors, request_buffer, null, byte_count) orelse {
        const err = device.stats.last_error;
        recordReadFailure(device, err);
        return err;
    };
    scheduleDeviceQueue(device, use_runtime_worker);
    const result = waitForRequest(device, request_id, requestTimeout(device));
    if (result.buffer_detached) release_bounce = false;
    if (!result.ok) return result.err;
    if (bounce) |memory| @memcpy(out[0..byte_count], memory[0..byte_count]);
    return .none;
}

pub fn write(index: usize, lba: u64, sectors: u16, data: []const u8) bool {
    const result = writeWithProgress(index, lba, sectors, data);
    return result.err == .none and result.sectors_completed == sectors;
}

pub fn writeWithProgress(index: usize, lba: u64, sectors: u16, data: []const u8) TransferResult {
    var pin = pinDevice(index) orelse return .{ .err = .invalid_request };
    defer unpinDevice(&pin);
    const device = pin.device;
    if (device.write_fn == null or !device.writable) {
        recordWriteFailure(device, .no_writer);
        return .{ .err = .no_writer };
    }
    if (validateRequest(device, lba, sectors, data.len)) |err| {
        recordWriteFailure(device, err);
        return .{ .err = err };
    }
    var iterator = block_split.Iterator.init(sectors, device.max_sectors_per_request, device.sector_size) orelse {
        recordWriteFailure(device, .invalid_request);
        return .{ .err = .invalid_request };
    };
    var completed: u16 = 0;
    while (iterator.next()) |chunk| {
        const err = writeChunk(
            device,
            lba + @as(u64, chunk.sector_offset),
            chunk.sectors,
            data[chunk.byte_offset .. chunk.byte_offset + chunk.byte_count],
        );
        if (err != .none) return .{ .sectors_completed = completed, .err = err };
        completed += chunk.sectors;
    }
    return .{ .sectors_completed = completed };
}

fn writeChunk(device: *Device, lba: u64, sectors: u16, data: []const u8) Error {
    const byte_count = @as(usize, sectors) * @as(usize, device.sector_size);
    const use_runtime_worker = runtimeWorkerReady();
    const bounce = if (use_runtime_worker)
        heap.alloc(byte_count, 16) orelse {
            recordWriteFailure(device, .busy);
            return .busy;
        }
    else
        null;
    var release_bounce = true;
    defer if (release_bounce) if (bounce) |memory| {
        _ = heap.free(memory);
    };
    if (bounce) |memory| @memcpy(memory[0..byte_count], data[0..byte_count]);
    const request_buffer = if (bounce) |memory| memory.ptr else data.ptr;
    const request_id = enqueueRequest(device, .write, lba, sectors, null, request_buffer, byte_count) orelse {
        const err = device.stats.last_error;
        recordWriteFailure(device, err);
        return err;
    };
    scheduleDeviceQueue(device, use_runtime_worker);
    const result = waitForRequest(device, request_id, requestTimeout(device));
    if (result.buffer_detached) release_bounce = false;
    return if (result.ok) .none else result.err;
}

pub fn flush(index: usize) bool {
    var pin = pinDevice(index) orelse return false;
    defer unpinDevice(&pin);
    const device = pin.device;
    if (device.flush_fn == null) return true;
    const request_id = enqueueRequest(device, .flush, 0, 0, null, null, 0) orelse {
        recordFlushFailure(device, device.stats.last_error);
        return false;
    };
    scheduleDeviceQueue(device, runtimeWorkerReady());
    return waitForRequest(device, request_id, requestTimeout(device)).ok;
}

fn enqueueRequest(device: *Device, kind: RequestKind, lba: u64, sectors: u16, buffer: ?[*]u8, const_buffer: ?[*]const u8, buffer_len: usize) ?u64 {
    const timeout = requestTimeout(device);
    const forever = timeout == sync.WAIT_FOREVER;
    const wait_start = timer.tickCount();
    const finite_deadline = if (forever) std.math.maxInt(u64) else timer.deadlineAfter(wait_start, timeout);
    while (true) {
        const locked = lockDevice(device);
        if (findFreeSlot(device)) |slot_index| {
            device.stats.next_request_id +%= 1;
            const id = device.stats.next_request_id;
            device.request_slots[slot_index] = .{
                .state = .queued,
                .id = id,
                .kind = kind,
                .lba = lba,
                .sectors = sectors,
                .buffer = buffer,
                .const_buffer = const_buffer,
                .buffer_len = buffer_len,
                .submit_tick = timer.tickCount(),
            };
            device.stats.queued_requests +%= 1;
            updateQueueHighWater(device);
            unlockDevice(device, locked);
            return id;
        }

        device.stats.queue_full_waits +%= 1;
        unlockDevice(device, locked);
        if (!canBlockOnStorage()) {
            recordQueueBackpressure(device, .busy);
            return null;
        }

        const now = timer.tickCount();
        if (!forever and now >= finite_deadline) {
            recordQueueBackpressure(device, .timeout);
            return null;
        }
        // A wake for a competing submitter must not restart this request's
        // relative timeout. Wait only the remaining absolute admission budget.
        const remaining = if (forever) sync.WAIT_FOREVER else finite_deadline - now;
        const wait_result = device.slot_available.wait(remaining, "block-slot");
        if (wait_result == .signaled) continue;
        recordQueueBackpressure(device, if (wait_result == .timeout) .timeout else .busy);
        return null;
    }
}

fn scheduleDeviceQueue(device: *Device, use_runtime_worker: bool) void {
    // The caller chooses the execution owner before publishing any buffer.
    // Re-checking here could switch an unbounced early-boot pointer to the
    // asynchronous worker if that worker became live between enqueue/wake.
    if (use_runtime_worker) {
        runtime_summary.worker_wakeups +%= 1;
        runtime_event.signal();
        return;
    }

    _ = pumpDeviceQueue(device, .boot_inline);
}

fn pumpDeviceQueue(device: *Device, mode: ExecutionMode) bool {
    var did_work = false;
    while (beginNextRequest(device, mode)) |request| {
        did_work = true;
        const result = executeRequest(device, request);
        finishRequest(device, request, result.ok, result.err);
    }
    return did_work;
}

fn beginNextRequest(device: *Device, mode: ExecutionMode) ?RequestExecution {
    const locked = lockDevice(device);
    const slot_index = findQueuedSlot(device) orelse {
        unlockDevice(device, locked);
        return null;
    };
    const start_tick = timer.tickCount();
    var slot = &device.request_slots[slot_index];
    slot.state = .active;
    slot.start_tick = start_tick;
    device.active_executions +|= 1;
    device.stats.dequeued_requests +%= 1;
    device.stats.active_request = snapshotFromSlot(slot.*);
    device.state = .busy;
    const request = RequestExecution{
        .slot_index = slot_index,
        .id = slot.id,
        .kind = slot.kind,
        .lba = slot.lba,
        .sectors = slot.sectors,
        .buffer = slot.buffer,
        .const_buffer = slot.const_buffer,
        .buffer_len = slot.buffer_len,
        .start_tick = start_tick,
        .mode = mode,
    };
    switch (mode) {
        .boot_inline => {
            device.stats.boot_inline_requests +%= 1;
            runtime_summary.boot_inline_requests +%= 1;
        },
        .runtime_worker => {
            device.stats.worker_requests +%= 1;
            runtime_summary.worker_runtime_requests +%= 1;
        },
    }
    unlockDevice(device, locked);
    return request;
}

fn executeRequest(device: *Device, request: RequestExecution) RequestResult {
    switch (request.kind) {
        .read => {
            const out_ptr = request.buffer orelse return .{ .err = .buffer_too_small };
            const ok = device.read_fn(device.ctx, request.lba, request.sectors, out_ptr[0..request.buffer_len]);
            return .{ .ok = ok, .err = if (ok) .none else .backend_read };
        },
        .write => {
            const write_fn = device.write_fn orelse return .{ .err = .no_writer };
            const data_ptr = request.const_buffer orelse return .{ .err = .buffer_too_small };
            const ok = write_fn(device.ctx, request.lba, request.sectors, data_ptr[0..request.buffer_len]);
            return .{ .ok = ok, .err = if (ok) .none else .backend_write };
        },
        .flush => {
            const flush_fn = device.flush_fn orelse return .{ .ok = true };
            const ok = flush_fn(device.ctx);
            return .{ .ok = ok, .err = if (ok) .none else .backend_flush };
        },
        .none => return .{ .err = .invalid_request },
    }
}

fn finishRequest(device: *Device, request: RequestExecution, ok: bool, err: Error) void {
    var detached_buffer: ?[]u8 = null;
    const locked = lockDevice(device);
    if (request.slot_index >= effectiveQueueDepth(device)) {
        unlockDevice(device, locked);
        return;
    }
    var slot = &device.request_slots[request.slot_index];
    if (slot.id != request.id or slot.state != .active) {
        unlockDevice(device, locked);
        return;
    }
    // Only the exact live execution owns one active count. A stale or double
    // completion must not make quiescence visible while real I/O still runs.
    if (device.active_executions != 0) device.active_executions -= 1;
    const complete_tick = timer.tickCount();
    const latency = if (complete_tick >= request.start_tick) complete_tick - request.start_tick else 0;
    const timed_out = slot.timeout_requested;
    const caller_detached = slot.caller_detached;
    if (caller_detached and slot.backend_owns_buffer) {
        detached_buffer = switch (request.kind) {
            .read => if (request.buffer) |ptr| ptr[0..request.buffer_len] else null,
            .write => if (request.const_buffer) |ptr| @constCast(ptr[0..request.buffer_len]) else null,
            else => null,
        };
    }
    slot.state = .completed;
    slot.ok = if (timed_out) false else ok;
    slot.err = if (timed_out) .timeout else err;
    slot.complete_tick = complete_tick;
    device.stats.last_request = snapshotFromSlot(slot.*);
    device.stats.active_request = .{};
    device.stats.completion_last_ticks = latency;
    device.stats.completion_total_ticks +%= latency;
    if (latency > device.stats.completion_max_ticks) device.stats.completion_max_ticks = latency;
    if (timed_out) {
        if (!caller_detached) recordRequestFailure(device, request.kind, .timeout);
    } else if (ok) {
        recordRequestSuccess(device, request.kind, request.sectors);
    } else {
        recordRequestFailure(device, request.kind, err);
    }
    if (!caller_detached) {
        _ = device.completion_available.wakeAll();
        device.stats.completion_signals +%= 1;
        runtime_summary.completion_signals +%= 1;
    }
    switch (request.mode) {
        .boot_inline => {
            device.stats.boot_inline_completions +%= 1;
            runtime_summary.boot_inline_completions +%= 1;
        },
        .runtime_worker => {
            device.stats.worker_completions +%= 1;
            runtime_summary.worker_runtime_completions +%= 1;
        },
    }
    if (caller_detached) {
        slot.* = .{};
        _ = device.slot_available.wakeOne();
    }
    unlockDevice(device, locked);
    if (detached_buffer) |memory| _ = heap.free(memory);
}

// Storage-wait watchdog (0.60.20): a device registered without a timeout
// used to park the requester with WAIT_FOREVER.  A single wedged request
// (worker stuck in the driver, lost completion) then froze the whole
// system, because the requester holds the FS request path while it sleeps.
// Now every wait runs in bounded observation slices.  A queued request can
// be cancelled safely when its caller timeout expires.  An active runtime
// request detaches its caller: the worker retains the non-pageable bounce
// buffer and frees it only after the backend callback returns.  This bounds
// caller latency without an unsafe early free or slot reuse. WAIT_FOREVER
// remains an ownership-safe forever wait and keeps the backed-off reports.
const WATCHDOG_SLICE_TICKS: u64 = 5 * @as(u64, timer.DEFAULT_HZ);

fn shouldReportWatchdogSlice(slices: u32) bool {
    return slices == 1 or (slices & (slices - 1)) == 0;
}

fn watchdogReport(
    device: *Device,
    request_id: u64,
    slices: u32,
    incident_token: *diag_screen.IncidentToken,
) void {
    // COM1/LOGSVC may be unreachable (LOGSVC itself needs storage), so paint
    // straight to the framebuffer. Do not enter crash mode merely because a
    // slow removable device crossed one observation slice.
    // First line BEFORE taking the device lock: if the queue lock itself is
    // wedged, at least the stall marker reaches the framebuffer.
    if (!incident_token.*.valid()) {
        incident_token.* = diag_screen.beginResolvableIncident();
    }
    diag_screen.write("[BLOCK] stall dev=");
    diag_screen.write(device.name);
    diag_screen.write(" slice=");
    diag_screen.writeDec(slices);
    diag_screen.endLine();
    k.puts("[BLOCK] watchdog: stall dev=");
    k.puts(device.name);
    k.puts(" slice=");
    putDec(slices);
    k.puts("\n");
    var snapshot: RequestSlot = .{};
    var have_slot = false;
    const locked = device.queue_lock.tryLock();
    if (locked) {
        if (findSlotById(device, request_id)) |slot_index| {
            snapshot = device.request_slots[slot_index];
            have_slot = true;
        }
    }
    const dev_state = device.state;
    unlockDevice(device, locked);

    diag_screen.write("[BLOCK] request dev=");
    diag_screen.write(device.name);
    diag_screen.write(" state=");
    diag_screen.write(@tagName(dev_state));
    if (!locked) {
        diag_screen.write(" queue_lock=busy");
    } else if (have_slot) {
        diag_screen.write(" req=");
        diag_screen.write(@tagName(snapshot.kind));
        diag_screen.write(" slot=");
        diag_screen.write(@tagName(snapshot.state));
        diag_screen.write(" lba=");
        diag_screen.writeDec(snapshot.lba);
        diag_screen.write(" sectors=");
        diag_screen.writeDec(snapshot.sectors);
    } else {
        diag_screen.write(" req=unknown");
    }
    diag_screen.endLine();

    k.puts("[BLOCK] watchdog: request stalled dev=");
    k.puts(device.name);
    k.puts(" driver=");
    k.puts(device.driver);
    k.puts(" state=");
    k.puts(@tagName(dev_state));
    k.puts(" slice=");
    putDec(slices);
    if (have_slot) {
        k.puts(" req=");
        k.puts(@tagName(snapshot.kind));
        k.puts(" slot=");
        k.puts(@tagName(snapshot.state));
        k.puts(" lba=");
        putDec(snapshot.lba);
        k.puts(" sectors=");
        putDec(snapshot.sectors);
        k.puts(" submit_tick=");
        putDec(snapshot.submit_tick);
        k.puts(" start_tick=");
        putDec(snapshot.start_tick);
        k.puts(" now=");
        putDec(timer.tickCount());
    } else {
        k.puts(" req=unknown");
    }
    k.puts("\n");
}

/// Deadman probe (0.60.20): is any request slot executing in a driver
/// callback for longer than `threshold` ticks?  The runtime worker calls
/// the driver synchronously; a slot that stays .active that long means the
/// worker is stuck (or spinning) INSIDE the driver -- invisible to
/// blocked-task scans.  Racy read-only scan, crash-class trigger only.
pub fn hasStalledExecution(now: u64, threshold: u64) bool {
    var index: usize = 0;
    while (index < device_slot_count) : (index += 1) {
        const slot_entry = &devices[index];
        if (!slot_entry.used) continue;
        const device = &slot_entry.device;
        for (device.request_slots) |slot| {
            if (slot.state != .active) continue;
            if (now < slot.start_tick) continue;
            if (now - slot.start_tick > threshold) return true;
        }
    }
    return false;
}

/// Framebuffer-direct variant of dumpStalledExecutions (0.60.20).
pub fn dumpStalledToDiag(now: u64, threshold: u64) void {
    var index: usize = 0;
    while (index < device_slot_count) : (index += 1) {
        const slot_entry = &devices[index];
        if (!slot_entry.used) continue;
        const device = &slot_entry.device;
        for (device.request_slots) |slot| {
            if (slot.state != .active) continue;
            if (now < slot.start_tick or now - slot.start_tick <= threshold) continue;
            diag_screen.write("[BLOCK] stalled exec dev=");
            diag_screen.write(device.name);
            diag_screen.write(" drv=");
            diag_screen.write(device.driver);
            diag_screen.write(" req=");
            diag_screen.write(@tagName(slot.kind));
            diag_screen.write(" lba=");
            diag_screen.writeDec(slot.lba);
            diag_screen.write(" active_for=");
            diag_screen.writeDec(now - slot.start_tick);
            diag_screen.endLine();
        }
    }
}

/// Paints every stalled execution (see hasStalledExecution) to the log.
pub fn dumpStalledExecutions(now: u64, threshold: u64) void {
    var index: usize = 0;
    while (index < device_slot_count) : (index += 1) {
        const slot_entry = &devices[index];
        if (!slot_entry.used) continue;
        const device = &slot_entry.device;
        for (device.request_slots) |slot| {
            if (slot.state != .active) continue;
            if (now < slot.start_tick or now - slot.start_tick <= threshold) continue;
            k.puts("[BLOCK] stalled execution dev=");
            k.puts(device.name);
            k.puts(" driver=");
            k.puts(device.driver);
            k.puts(" req=");
            k.puts(@tagName(slot.kind));
            k.puts(" lba=");
            putDec(slot.lba);
            k.puts(" sectors=");
            putDec(slot.sectors);
            k.puts(" active_for=");
            putDec(now - slot.start_tick);
            k.puts(" ticks\r\n");
        }
    }
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

fn waitForRequest(device: *Device, request_id: u64, timeout: u64) RequestResult {
    var counted_wait = false;
    var watchdog_slices: u32 = 0;
    var incident_token: diag_screen.IncidentToken = .{};
    defer if (incident_token.valid()) {
        // Success and classified failure both end this reporter's lifetime.
        // resolveIncident retains the evidence but prevents an abandoned
        // token from suppressing every later storage diagnosis.
        _ = diag_screen.resolveIncident(incident_token);
    };
    const forever = timeout == sync.WAIT_FOREVER;
    const wait_start = timer.tickCount();
    const finite_deadline = if (forever) std.math.maxInt(u64) else timer.deadlineAfter(wait_start, timeout);
    while (true) {
        const locked = lockDevice(device);
        const slot_index = findSlotById(device, request_id) orelse {
            unlockDevice(device, locked);
            return .{ .err = .invalid_request };
        };
        if (!counted_wait) {
            device.stats.completion_waits +%= 1;
            counted_wait = true;
        }
        const slot = &device.request_slots[slot_index];
        if (slot.state == .completed) {
            const result = RequestResult{ .ok = slot.ok, .err = slot.err };
            device.request_slots[slot_index] = .{};
            _ = device.slot_available.wakeOne();
            unlockDevice(device, locked);
            return result;
        }
        unlockDevice(device, locked);

        if (!canBlockOnStorage()) {
            _ = pumpDeviceQueue(device, .boot_inline);
            continue;
        }

        if (forever) {
            const wait_result = device.completion_available.wait(WATCHDOG_SLICE_TICKS, "block-completion");
            if (wait_result == .signaled) continue;
            watchdog_slices += 1;
            if (shouldReportWatchdogSlice(watchdog_slices)) {
                watchdogReport(device, request_id, watchdog_slices, &incident_token);
            }
            continue;
        }
        // A foreign completion wakeup must not restart the caller's entire
        // timeout.  Every pass waits only the remaining absolute budget.
        const now = timer.tickCount();
        if (now < finite_deadline) {
            const wait_result = device.completion_available.wait(finite_deadline - now, "block-completion");
            if (wait_result == .signaled) continue;
        }
        const timeout_locked = lockDevice(device);
        if (findSlotById(device, request_id)) |timeout_slot_index| {
            var timeout_slot = &device.request_slots[timeout_slot_index];
            switch (timeout_slot.state) {
                .queued => {
                    recordRequestFailure(device, timeout_slot.kind, .timeout);
                    timeout_slot.* = .{};
                    _ = device.slot_available.wakeOne();
                    device.stats.completion_timeouts +%= 1;
                    unlockDevice(device, timeout_locked);
                    return .{ .err = .timeout };
                },
                .active => {
                    if (!timeout_slot.timeout_requested) {
                        timeout_slot.timeout_requested = true;
                        device.stats.completion_timeouts +%= 1;
                    }
                    timeout_slot.caller_detached = true;
                    timeout_slot.backend_owns_buffer = timeout_slot.kind == .read or timeout_slot.kind == .write;
                    recordRequestFailure(device, timeout_slot.kind, .timeout);
                    const buffer_detached = timeout_slot.backend_owns_buffer;
                    unlockDevice(device, timeout_locked);
                    watchdog_slices += 1;
                    if (shouldReportWatchdogSlice(watchdog_slices)) {
                        watchdogReport(device, request_id, watchdog_slices, &incident_token);
                    }
                    return .{ .err = .timeout, .buffer_detached = buffer_detached };
                },
                .completed => {
                    unlockDevice(device, timeout_locked);
                    continue;
                },
                .free => {},
            }
        } else {
            device.stats.timeout_failures +%= 1;
            device.stats.last_error = .timeout;
        }
        device.stats.completion_timeouts +%= 1;
        unlockDevice(device, timeout_locked);
        return .{ .err = .timeout };
    }
}

fn recordRequestSuccess(device: *Device, kind: RequestKind, sectors: u16) void {
    device.stats.last_error = .none;
    device.stats.completions += 1;
    device.state = .active;
    switch (kind) {
        .read => {
            device.stats.read_ops += 1;
            device.stats.read_sectors += sectors;
        },
        .write => {
            device.stats.write_ops += 1;
            device.stats.write_sectors += sectors;
        },
        .flush => device.stats.flush_ops += 1,
        .none => {},
    }
}

fn recordRequestFailure(device: *Device, kind: RequestKind, err: Error) void {
    device.stats.last_error = err;
    if (err == .timeout) device.stats.timeout_failures += 1;
    device.state = .failed;
    switch (kind) {
        .read => device.stats.read_failures += 1,
        .write => device.stats.write_failures += 1,
        .flush => device.stats.flush_failures += 1,
        .none => {},
    }
}

fn recordQueueBackpressure(device: *Device, err: Error) void {
    const locked = lockDevice(device);
    device.stats.queue_full_rejections +%= 1;
    device.stats.busy_rejections +%= 1;
    if (err == .timeout) device.stats.timeout_failures +%= 1;
    device.stats.last_error = err;
    unlockDevice(device, locked);
}

fn recordReadFailure(device: *Device, err: Error) void {
    device.stats.read_failures += 1;
    device.stats.last_error = err;
    if (err == .timeout) device.stats.timeout_failures += 1;
}

fn recordWriteFailure(device: *Device, err: Error) void {
    device.stats.write_failures += 1;
    device.stats.last_error = err;
    if (err == .timeout) device.stats.timeout_failures += 1;
}

fn recordFlushFailure(device: *Device, err: Error) void {
    device.stats.flush_failures += 1;
    device.stats.last_error = err;
    if (err == .timeout) device.stats.timeout_failures += 1;
}

fn validateRequest(device: *const Device, lba: u64, sectors: u16, bytes: usize) ?Error {
    if (sectors == 0) return .invalid_request;
    if (@as(u64, sectors) > std.math.maxInt(u64) - lba) return .out_of_range;
    if (bytes < @as(usize, sectors) * @as(usize, device.sector_size)) return .buffer_too_small;
    if (device.sector_count != 0) {
        if (lba >= device.sector_count) return .out_of_range;
        if (@as(u64, sectors) > device.sector_count - lba) return .out_of_range;
    }
    return null;
}

fn requestTimeout(device: *const Device) u64 {
    return if (device.timeout_ticks == 0) sync.WAIT_FOREVER else device.timeout_ticks;
}

fn effectiveQueueDepth(device: *const Device) usize {
    const configured: usize = if (device.queue_depth == 0) 1 else @intCast(device.queue_depth);
    return @min(configured, MAX_REQUEST_QUEUE_DEPTH);
}

fn findFreeSlot(device: *Device) ?usize {
    const limit = effectiveQueueDepth(device);
    var i: usize = 0;
    while (i < limit) : (i += 1) {
        if (device.request_slots[i].state == .free) return i;
    }
    return null;
}

fn findQueuedSlot(device: *Device) ?usize {
    const limit = effectiveQueueDepth(device);
    var i: usize = 0;
    while (i < limit) : (i += 1) {
        if (device.request_slots[i].state == .queued) return i;
    }
    return null;
}

fn findSlotById(device: *Device, request_id: u64) ?usize {
    const limit = effectiveQueueDepth(device);
    var i: usize = 0;
    while (i < limit) : (i += 1) {
        if (device.request_slots[i].state != .free and device.request_slots[i].id == request_id) return i;
    }
    return null;
}

fn countUsedSlots(device: *const Device) u32 {
    const limit = effectiveQueueDepth(device);
    var used: u32 = 0;
    var i: usize = 0;
    while (i < limit) : (i += 1) {
        if (device.request_slots[i].state != .free) used += 1;
    }
    return used;
}

fn updateQueueHighWater(device: *Device) void {
    const used = countUsedSlots(device);
    if (used > device.stats.queue_high_water) device.stats.queue_high_water = used;
}

fn snapshotFromSlot(slot: RequestSlot) RequestSnapshot {
    return .{
        .id = slot.id,
        .kind = slot.kind,
        .lba = slot.lba,
        .sectors = slot.sectors,
    };
}

fn lockDevice(device: *Device) bool {
    if (!canBlockOnStorage()) return false;
    // Bounded slices instead of a silent forever-wait (0.60.20): if the
    // queue lock is wedged, say so loudly every slice and keep trying.
    var slices: u32 = 0;
    var incident_token: diag_screen.IncidentToken = .{};
    while (true) {
        if (device.queue_lock.lock(WATCHDOG_SLICE_TICKS)) {
            if (incident_token.valid()) _ = diag_screen.resolveIncident(incident_token);
            return true;
        }
        slices += 1;
        if (!incident_token.valid()) {
            incident_token = diag_screen.beginResolvableIncident();
        }
        diag_screen.write("[BLOCK] queue-lock stall dev=");
        diag_screen.write(device.name);
        diag_screen.write(" slice=");
        diag_screen.writeDec(slices);
        diag_screen.endLine();
        k.puts("[BLOCK] queue-lock stall dev=");
        k.puts(device.name);
        k.puts(" slice=");
        putDec(slices);
        k.puts("\r\n");
    }
}

fn unlockDevice(device: *Device, locked: bool) void {
    if (locked) _ = device.queue_lock.unlock();
}

fn canBlockOnStorage() bool {
    return scheduler.currentId() != null;
}

fn workerMain() callconv(.c) void {
    while (true) {
        if (pumpAllRuntimeQueues()) continue;
        runtime_summary.worker_idle_waits +%= 1;
        _ = runtime_event.waitResult(scheduler.WAIT_FOREVER);
    }
}

fn pumpAllRuntimeQueues() bool {
    runtime_summary.worker_queue_scans +%= 1;
    var did_work = false;
    var index: usize = 0;
    while (index < device_slot_count) : (index += 1) {
        if (pumpRuntimeDevice(index)) did_work = true;
    }
    if (did_work) runtime_summary.worker_runs +%= 1;
    return did_work;
}

fn pumpRuntimeDevice(index: usize) bool {
    var pin = pinDevice(index) orelse return false;
    defer unpinDevice(&pin);
    return pumpDeviceQueue(pin.device, .runtime_worker);
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
