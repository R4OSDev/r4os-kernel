// Kanalbasierte Kernel-IPC fuer die Netz-Service-Bruecke. Kanal- und
// Slotidentitaet bleiben statisch, waehrend Handlerarbeit nach Start der
// Task-Runtime auf einem eigenen Worker laeuft.
const builtin = @import("builtin");
const k = @import("log.zig");
const irq_router = @import("irq_router.zig");
const monotonic = @import("../platform/monotonic.zig");
const queue_model = @import("service_ipc_queue.zig");
const sched_task = @import("../sched/task.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const task_context = @import("../sched/task_context.zig");
const timer = @import("timer.zig");

pub const MAX_CHANNELS: usize = 16;
pub const QUEUE_DEPTH: usize = 8;
// 0.56.37: 1024 -> 4096. MUSS mit SDK abi.ipc_max_message_size
// uebereinstimmen.
pub const MAX_MESSAGE_SIZE: usize = 4096;
pub const WAIT_FOREVER: u64 = sync.WAIT_FOREVER;

pub const CHANNEL_ECHO: u32 = 1;
pub const CHANNEL_NET_DHCP: u32 = 2;
pub const CHANNEL_NET_DNS: u32 = 3;
pub const CHANNEL_NET_TCP: u32 = 4;
pub const CHANNEL_NET_UDP: u32 = 5;

pub const ServiceHandler = *const fn (channel_id: u32, request: []const u8, response: []u8) i32;

const Message = struct {
    meta: queue_model.SlotMeta = .{},
    channel_generation: u64 = 0,
    handler: ?ServiceHandler = null,
    completion: sync.Completion = sync.Completion.init(),
    submitted_at: monotonic.Stamp = .{},
    started_at: monotonic.Stamp = .{},
    data: [MAX_MESSAGE_SIZE]u8 = .{0} ** MAX_MESSAGE_SIZE,
};

const Lifetime = struct {
    send_calls: u64 = 0,
    recv_calls: u64 = 0,
    request_calls: u64 = 0,
    handler_queued: u64 = 0,
    handler_started: u64 = 0,
    handler_completed: u64 = 0,
    handler_failures: u64 = 0,
    handler_direct: u64 = 0,
    handler_waits: u64 = 0,
    handler_wait_timeouts: u64 = 0,
    handler_queue_ns: u64 = 0,
    handler_queue_max_ns: u64 = 0,
    handler_run_ns: u64 = 0,
    handler_run_max_ns: u64 = 0,
    handler_e2e_ns: u64 = 0,
    handler_e2e_max_ns: u64 = 0,
    timing_unavailable: u64 = 0,
    request_bytes: u64 = 0,
    response_bytes: u64 = 0,
    payload_copy_bytes: u64 = 0,
    payload_clear_bytes: u64 = 0,
    queue_full: u64 = 0,
    queue_empty: u64 = 0,
    admission_waits: u64 = 0,
    admission_timeouts: u64 = 0,
    recv_buffer_small: u64 = 0,
    response_searches: u64 = 0,
    response_search_slots: u64 = 0,
    stale_drops: u64 = 0,
    lock_contentions: u64 = 0,
};

const Channel = struct {
    // Die Adresse von Lock, Semaphore, Completion und Queue bleibt ueber die
    // gesamte Kernel-Laufzeit stabil. resetartige Ganzstrukturzuweisungen
    // sind deshalb verboten, sobald die Task-Runtime laeuft.
    lock: sync.Mutex = sync.Mutex.initClass("service-ipc-channel", sync.LockRank.service_endpoint, .sleepable),
    slots_available: sync.Semaphore = sync.Semaphore.init(@intCast(QUEUE_DEPTH), @intCast(QUEUE_DEPTH)),
    generation: u64 = 1,
    next_sequence: u64 = 1,
    active: bool = false,
    id: u32 = 0,
    opens: u64 = 0,
    closes: u64 = 0,
    sends: u64 = 0,
    receives: u64 = 0,
    drops: u64 = 0,
    service_name: []const u8 = "",
    handler: ?ServiceHandler = null,
    queue: [QUEUE_DEPTH]Message = .{Message{}} ** QUEUE_DEPTH,
    lifetime: Lifetime = .{},
};

pub const Summary = extern struct {
    max_channels: u32 = @intCast(MAX_CHANNELS),
    active_channels: u32 = 0,
    max_message_size: u32 = @intCast(MAX_MESSAGE_SIZE),
    queue_depth: u32 = @intCast(QUEUE_DEPTH),
    sends: u64 = 0,
    receives: u64 = 0,
    drops: u64 = 0,
    errors: u64 = 0,
    echo_tests: u64 = 0,
};

pub const ChannelInfo = extern struct {
    id: u32 = 0,
    active: u32 = 0,
    queued: u32 = 0,
    queue_depth: u32 = @intCast(QUEUE_DEPTH),
    max_message_size: u32 = @intCast(MAX_MESSAGE_SIZE),
    has_handler: u32 = 0,
    reserved0: u32 = 0,
    reserved1: u32 = 0,
    opens: u64 = 0,
    closes: u64 = 0,
    sends: u64 = 0,
    receives: u64 = 0,
    drops: u64 = 0,
    name: [16]u8 = .{0} ** 16,
};

pub const PerformanceSummary = struct {
    worker_started: u32 = 0,
    worker_task_id: u32 = 0,
    active_channels: u32 = 0,
    queue_used: u32 = 0,
    queue_ready: u32 = 0,
    queue_running: u32 = 0,
    queue_limit: u32 = 0,
    reserved0: u32 = 0,
    send_calls: u64 = 0,
    recv_calls: u64 = 0,
    request_calls: u64 = 0,
    handler_queued: u64 = 0,
    handler_started: u64 = 0,
    handler_completed: u64 = 0,
    handler_failures: u64 = 0,
    handler_direct: u64 = 0,
    handler_waits: u64 = 0,
    handler_wait_timeouts: u64 = 0,
    handler_queue_ns: u64 = 0,
    handler_queue_max_ns: u64 = 0,
    handler_run_ns: u64 = 0,
    handler_run_max_ns: u64 = 0,
    handler_e2e_ns: u64 = 0,
    handler_e2e_max_ns: u64 = 0,
    timing_unavailable: u64 = 0,
    request_bytes: u64 = 0,
    response_bytes: u64 = 0,
    payload_copy_bytes: u64 = 0,
    payload_clear_bytes: u64 = 0,
    queue_full: u64 = 0,
    queue_empty: u64 = 0,
    admission_waits: u64 = 0,
    admission_timeouts: u64 = 0,
    recv_buffer_small: u64 = 0,
    response_searches: u64 = 0,
    response_search_slots: u64 = 0,
    stale_drops: u64 = 0,
    lock_contentions: u64 = 0,
    irq_denied: u64 = 0,
    worker_idle_waits: u64 = 0,
    worker_wakes: u64 = 0,
};

const ChannelGuard = struct {
    channel: *Channel,
    locked: bool = false,
    admitted: bool = false,
};

const HandlerTarget = struct {
    channel_index: usize,
    channel_generation: u64,
    slot_index: usize,
    slot_generation: u64,
    completion: *sync.Completion,
    request_len: usize,
    deadline: u64,
    forever: bool,
};

const HandlerAdmission = struct {
    target: HandlerTarget,
    unwind: task_context.UnwindToken,
};

const WorkerTarget = struct {
    channel_index: usize,
    channel_generation: u64,
    slot_index: usize,
    slot_generation: u64,
    handler: ServiceHandler,
};

var initialized = false;
var worker_started = false;
var worker_task_id: u32 = 0;
var worker_cursor: usize = 0;
var worker_event = sync.EventV2.initMode(false, .auto_reset);
var channels: [MAX_CHANNELS]Channel = .{Channel{}} ** MAX_CHANNELS;
var total_sends: u64 = 0;
var total_receives: u64 = 0;
var total_drops: u64 = 0;
var total_errors: u64 = 0;
var echo_tests: u64 = 0;
var irq_denied: u64 = 0;
var worker_idle_waits: u64 = 0;
var worker_wakes: u64 = 0;

pub fn init() void {
    if (initialized) return;
    initialized = true;
    _ = open(CHANNEL_ECHO);
}

pub fn startRuntimeWorker() bool {
    if (worker_started) return true;
    const worker = sched_task.createKernelThread("ipc-worker", workerMain) orelse return false;
    worker_task_id = worker.id;
    worker_started = true;
    return true;
}

pub fn registerService(channel_id: u32, name: []const u8, handler: ServiceHandler) i32 {
    if (denyIrq()) return fail();
    const idx = index(channel_id) orelse return fail();
    var guard = lockChannel(&channels[idx]) orelse return fail();
    defer unlockChannel(&guard);
    const ch = guard.channel;
    if (!ch.active) activateLocked(ch, channel_id);
    if (ch.handler != handler or !memEql(ch.service_name, name)) {
        ch.generation = nextNonZero(ch.generation);
    }
    ch.service_name = name;
    ch.handler = handler;
    return 0;
}

pub fn open(channel_id: u32) i32 {
    if (denyIrq()) return fail();
    const idx = index(channel_id) orelse return fail();
    var guard = lockChannel(&channels[idx]) orelse return fail();
    defer unlockChannel(&guard);
    const ch = guard.channel;
    if (!ch.active) activateLocked(ch, channel_id);
    ch.opens +%= 1;
    return @intCast(channel_id);
}

pub fn close(channel_id: u32) i32 {
    if (denyIrq()) return fail();
    const idx = index(channel_id) orelse return fail();
    var guard = lockChannel(&channels[idx]) orelse return fail();
    defer unlockChannel(&guard);
    const ch = guard.channel;
    if (!ch.active) return fail();
    // Kanal-IDs sind die stabilen ABI-Handles. Ein Close beendet die
    // Callerbeziehung, darf aber registrierte Handler oder bereits
    // publizierte Antworten nicht unter laufenden Verbrauchern recyceln.
    ch.closes +%= 1;
    return 0;
}

pub fn poll(channel_id: u32) i32 {
    if (denyIrq()) return fail();
    const idx = index(channel_id) orelse return fail();
    var guard = lockChannel(&channels[idx]) orelse return fail();
    defer unlockChannel(&guard);
    const ch = guard.channel;
    if (!ch.active) return fail();
    return @intCast(queue_model.countReady(&ch.queue));
}

pub fn send(channel_id: u32, payload: []const u8) i32 {
    if (payload.len > MAX_MESSAGE_SIZE) return fail();
    if (denyIrq()) return fail();
    const idx = index(channel_id) orelse return fail();

    var guard = lockChannel(&channels[idx]) orelse return fail();
    const ch = guard.channel;
    if (!ch.active) activateLocked(ch, channel_id);
    const handler_bound = ch.handler != null;
    const on_worker = currentIsWorker();
    unlockChannel(&guard);

    if (!handler_bound) return publishReady(channel_id, payload, .raw_ready, true, true);
    if (on_worker) return directSendFromWorker(channel_id, payload);
    // Der bestehende Raw-Send bleibt bei voller Queue nichtblockierend. Nach
    // erfolgreicher Aufnahme darf er wie schon der alte synchrone Handlerpfad
    // bis zum Handlerabschluss warten.
    return submitHandler(channel_id, payload, null, .queued_response, 0, WAIT_FOREVER);
}

// Zielgerichteter Kernelpfad fuer eine vollstaendige Handleranfrage. Die
// Antwort durchlaeuft kein gemeinsames Responsepostfach und kann deshalb
// weder von fremden/stalen Antworten verdraengt noch mehrfach kopiert und
// verworfen werden.
pub fn request(channel_id: u32, payload: []const u8, out: []u8, timeout_ticks: u64) i32 {
    if (payload.len > MAX_MESSAGE_SIZE or out.len > MAX_MESSAGE_SIZE) return fail();
    if (denyIrq()) return fail();
    if (currentIsWorker()) return directRequestFromWorker(channel_id, payload, out);
    return submitHandler(channel_id, payload, out, .direct_response, timeout_ticks, timeout_ticks);
}

pub fn recv(channel_id: u32, out: []u8) i32 {
    if (denyIrq()) return fail();
    const idx = index(channel_id) orelse return fail();
    var guard = lockChannel(&channels[idx]) orelse return fail();
    const ch = guard.channel;
    if (!ch.active) {
        unlockChannel(&guard);
        return fail();
    }
    ch.lifetime.recv_calls +%= 1;
    ch.lifetime.response_searches +%= 1;

    var release_permit = false;
    const result: i32 = switch (queue_model.receiveDecision(&ch.queue, out.len)) {
        .empty => result: {
            ch.lifetime.queue_empty +%= 1;
            break :result 0;
        },
        .drop_too_small => |decision| result: {
            ch.lifetime.response_search_slots +%= decision.selection.visits;
            ch.lifetime.recv_buffer_small +%= 1;
            ch.drops +%= 1;
            atomicAdd(&total_drops, 1);
            resetSlotLocked(&ch.queue[decision.selection.index]);
            release_permit = true;
            break :result -1;
        },
        .deliver => |selection| result: {
            ch.lifetime.response_search_slots +%= selection.visits;
            const slot = &ch.queue[selection.index];
            const len: usize = @intCast(slot.meta.len);
            if (len != 0) {
                @memcpy(out[0..len], slot.data[0..len]);
                ch.lifetime.payload_copy_bytes +%= len;
            }
            ch.receives +%= 1;
            atomicAdd(&total_receives, 1);
            resetSlotLocked(slot);
            release_permit = true;
            break :result @intCast(len);
        },
    };
    unlockChannel(&guard);
    if (release_permit) _ = ch.slots_available.release(1);
    if (result < 0) atomicAdd(&total_errors, 1);
    return result;
}

pub fn echoSmoke() bool {
    const payload = "R4IPC-ECHO";
    var out: [MAX_MESSAGE_SIZE]u8 = undefined;
    if (send(CHANNEL_ECHO, payload) != @as(i32, @intCast(payload.len))) return false;
    const got = recv(CHANNEL_ECHO, out[0..]);
    if (got != @as(i32, @intCast(payload.len))) return false;
    if (!memEql(out[0..payload.len], payload)) return false;
    atomicAdd(&echo_tests, 1);
    return true;
}

pub fn summary(out: *Summary) i32 {
    if (denyIrq()) return fail();
    var active: u32 = 0;
    var i: usize = 0;
    while (i < channels.len) : (i += 1) {
        var guard = lockChannel(&channels[i]) orelse continue;
        if (guard.channel.active) active += 1;
        unlockChannel(&guard);
    }
    out.* = .{
        .active_channels = active,
        .sends = atomicLoad(&total_sends),
        .receives = atomicLoad(&total_receives),
        .drops = atomicLoad(&total_drops),
        .errors = atomicLoad(&total_errors),
        .echo_tests = atomicLoad(&echo_tests),
    };
    return 1;
}

pub fn channelInfo(channel_id: u32, out: *ChannelInfo) i32 {
    if (denyIrq()) return fail();
    const idx = index(channel_id) orelse return fail();
    var guard = lockChannel(&channels[idx]) orelse return fail();
    defer unlockChannel(&guard);
    const ch = guard.channel;
    out.* = .{
        .id = channel_id,
        .active = if (ch.active) 1 else 0,
        .queued = @intCast(queue_model.countReady(&ch.queue)),
        .has_handler = if (ch.handler != null) 1 else 0,
        .opens = ch.opens,
        .closes = ch.closes,
        .sends = ch.sends,
        .receives = ch.receives,
        .drops = ch.drops,
    };
    copyFixed(out.name[0..], if (ch.service_name.len != 0) ch.service_name else if (channel_id == CHANNEL_ECHO) "echo" else "");
    return if (ch.active) 1 else 0;
}

pub fn performanceSummary() PerformanceSummary {
    var out = PerformanceSummary{
        .worker_started = if (worker_started) 1 else 0,
        .worker_task_id = worker_task_id,
        .queue_limit = @intCast(MAX_CHANNELS * QUEUE_DEPTH),
        .irq_denied = atomicLoad(&irq_denied),
        .worker_idle_waits = atomicLoad(&worker_idle_waits),
        .worker_wakes = atomicLoad(&worker_wakes),
    };
    var i: usize = 0;
    while (i < channels.len) : (i += 1) {
        var guard = lockChannel(&channels[i]) orelse continue;
        const ch = guard.channel;
        if (ch.active) out.active_channels += 1;
        out.queue_used +|= @intCast(queue_model.countUsed(&ch.queue));
        out.queue_ready +|= @intCast(queue_model.countReady(&ch.queue));
        out.queue_running +|= @intCast(queue_model.countRunning(&ch.queue));
        addLifetime(&out, ch.lifetime);
        unlockChannel(&guard);
    }
    return out;
}

// Kanal 0 liefert das Gesamtaggregat. 1..MAX_CHANNELS liefern dieselbe
// Momentaufnahme nur fuer den stabilen Kanal-Slot; dadurch kann die Diagnose
// einen Netzwerkdienst isoliert betrachten, ohne die Queue zu beeinflussen.
pub fn performanceSummaryFor(channel_id: u32, out: *PerformanceSummary) i32 {
    if (denyIrq()) return fail();
    if (channel_id == 0) {
        out.* = performanceSummary();
        return 1;
    }
    const idx = index(channel_id) orelse return fail();
    var result = PerformanceSummary{
        .worker_started = if (worker_started) 1 else 0,
        .worker_task_id = worker_task_id,
        .queue_limit = @intCast(QUEUE_DEPTH),
        .worker_idle_waits = atomicLoad(&worker_idle_waits),
        .worker_wakes = atomicLoad(&worker_wakes),
    };
    var guard = lockChannel(&channels[idx]) orelse return fail();
    const ch = guard.channel;
    if (ch.active) result.active_channels = 1;
    result.queue_used = @intCast(queue_model.countUsed(&ch.queue));
    result.queue_ready = @intCast(queue_model.countReady(&ch.queue));
    result.queue_running = @intCast(queue_model.countRunning(&ch.queue));
    addLifetime(&result, ch.lifetime);
    unlockChannel(&guard);
    out.* = result;
    return 1;
}

pub fn dumpStatus() void {
    var s: Summary = .{};
    _ = summary(&s);
    k.puts("IPC service bus\r\n");
    k.puts("  Model: synchronized channel queue, isolated handler worker\r\n");
    k.puts("  Limits: channels=");
    k.putDec(MAX_CHANNELS);
    k.puts(" queue_depth=");
    k.putDec(QUEUE_DEPTH);
    k.puts(" message_bytes=");
    k.putDec(MAX_MESSAGE_SIZE);
    k.puts("\r\n");
    k.puts("  Counters: active=");
    k.putDec(s.active_channels);
    k.puts(" sends=");
    k.putDec(s.sends);
    k.puts(" receives=");
    k.putDec(s.receives);
    k.puts(" drops=");
    k.putDec(s.drops);
    k.puts(" errors=");
    k.putDec(s.errors);
    k.puts(" echo_tests=");
    k.putDec(s.echo_tests);
    k.puts("\r\n");
    var channel_id: u32 = 1;
    while (channel_id <= MAX_CHANNELS) : (channel_id += 1) {
        var info: ChannelInfo = .{};
        if (channelInfo(channel_id, &info) <= 0) continue;
        k.puts("  channel ");
        k.putDec(info.id);
        if (info.name[0] != 0) {
            k.puts(" service=");
            k.puts(fixedText(info.name[0..]));
        }
        k.puts(": queued=");
        k.putDec(info.queued);
        k.puts(" opens=");
        k.putDec(info.opens);
        k.puts(" closes=");
        k.putDec(info.closes);
        k.puts(" sends=");
        k.putDec(info.sends);
        k.puts(" receives=");
        k.putDec(info.receives);
        k.puts(" drops=");
        k.putDec(info.drops);
        k.puts("\r\n");
    }
}

fn submitHandler(
    channel_id: u32,
    payload: []const u8,
    out: ?[]u8,
    mode: queue_model.Mode,
    admission_timeout_ticks: u64,
    completion_timeout_ticks: u64,
) i32 {
    if (!worker_started) return fail();
    const completion_forever = completion_timeout_ticks == WAIT_FOREVER;
    const completion_deadline = if (completion_forever) @as(u64, 0) else timer.deadlineAfterNow(completion_timeout_ticks);
    var admission = admitHandler(
        channel_id,
        payload,
        mode,
        admission_timeout_ticks,
        completion_deadline,
        completion_forever,
    ) orelse return fail();
    defer {
        if (admission.unwind.active) _ = task_context.leaveUnwind(admission.unwind);
    }
    worker_event.signal();

    const wait_ticks = remainingTicks(admission.target.deadline, admission.target.forever);
    const wait_result = admission.target.completion.wait(wait_ticks);
    return finishHandlerCall(admission.target, wait_result, out);
}

fn admitHandler(
    channel_id: u32,
    payload: []const u8,
    mode: queue_model.Mode,
    admission_timeout_ticks: u64,
    completion_deadline: u64,
    completion_forever: bool,
) ?HandlerAdmission {
    const idx = index(channel_id) orelse return null;
    const admission_forever = admission_timeout_ticks == WAIT_FOREVER;
    const admission_deadline = if (admission_forever) @as(u64, 0) else timer.deadlineAfterNow(admission_timeout_ticks);
    var guard = lockChannel(&channels[idx]) orelse return null;
    var ch = guard.channel;
    if (!ch.active) activateLocked(ch, channel_id);
    const identity_generation = ch.generation;
    const identity_handler = ch.handler orelse {
        unlockChannel(&guard);
        return null;
    };
    if (mode == .queued_response) {
        ch.lifetime.send_calls +%= 1;
    } else {
        ch.lifetime.request_calls +%= 1;
    }

    var unwind = task_context.enterUnwind();
    if (!unwind.admitted()) {
        unlockChannel(&guard);
        return null;
    }

    if (!ch.slots_available.tryAcquire()) {
        _ = task_context.leaveUnwind(unwind);
        unwind = .{};
        if (admission_timeout_ticks == 0) {
            ch.lifetime.queue_full +%= 1;
            ch.drops +%= 1;
            atomicAdd(&total_drops, 1);
            unlockChannel(&guard);
            return null;
        }
        ch.lifetime.admission_waits +%= 1;
        const remaining = remainingTicks(admission_deadline, admission_forever);
        const wait_result = ch.slots_available.acquireReleasingGuarded(
            remaining,
            releaseChannelForWait,
            &guard,
            &unwind,
        );
        guard = lockChannel(&channels[idx]) orelse {
            _ = channels[idx].slots_available.release(1);
            if (unwind.active) _ = task_context.leaveUnwind(unwind);
            return null;
        };
        ch = guard.channel;
        if (wait_result != .signaled) {
            if (wait_result == .timeout) ch.lifetime.admission_timeouts +%= 1;
            unlockChannel(&guard);
            if (unwind.active) _ = task_context.leaveUnwind(unwind);
            return null;
        }
        if (!ch.active or ch.generation != identity_generation or ch.handler != identity_handler) {
            unlockChannel(&guard);
            _ = ch.slots_available.release(1);
            if (unwind.active) _ = task_context.leaveUnwind(unwind);
            return null;
        }
    }

    const selection = queue_model.firstFree(&ch.queue) orelse {
        unlockChannel(&guard);
        _ = ch.slots_available.release(1);
        if (unwind.active) _ = task_context.leaveUnwind(unwind);
        return null;
    };
    const slot = &ch.queue[selection.index];
    prepareSlotLocked(ch, slot, mode, payload.len);
    slot.channel_generation = ch.generation;
    slot.handler = identity_handler;
    slot.submitted_at = monotonic.capture();
    if (payload.len != 0) {
        @memcpy(slot.data[0..payload.len], payload);
        ch.lifetime.payload_copy_bytes +%= payload.len;
    }
    ch.lifetime.request_bytes +%= payload.len;
    ch.lifetime.handler_queued +%= 1;
    ch.lifetime.handler_waits +%= 1;
    slot.meta.state = .handler_queued;
    const target = HandlerTarget{
        .channel_index = idx,
        .channel_generation = ch.generation,
        .slot_index = selection.index,
        .slot_generation = slot.meta.generation,
        .completion = &slot.completion,
        .request_len = payload.len,
        .deadline = completion_deadline,
        .forever = completion_forever,
    };
    unlockChannel(&guard);
    return .{ .target = target, .unwind = unwind };
}

fn finishHandlerCall(target: HandlerTarget, wait_result: sync.WaitResult, out: ?[]u8) i32 {
    var guard = lockChannel(&channels[target.channel_index]) orelse return fail();
    const ch = guard.channel;
    const slot = &ch.queue[target.slot_index];
    var release_permit = false;
    var result: i32 = -1;

    if (slot.meta.generation != target.slot_generation) {
        ch.lifetime.stale_drops +%= 1;
    } else if (wait_result != .signaled) {
        if (wait_result == .timeout) ch.lifetime.handler_wait_timeouts +%= 1;
        switch (queue_model.timeoutAction(slot.meta, target.slot_generation)) {
            .reset => {
                resetSlotLocked(slot);
                release_permit = true;
            },
            .abandon => slot.meta.abandoned = true,
            .none => {},
        }
    } else switch (queue_model.completionAction(slot.meta, target.slot_generation, if (out) |response| response.len else null)) {
        .discard_error => {
            resetSlotLocked(slot);
            release_permit = true;
        },
        .publish_ready => {
            slot.meta.state = .handler_ready;
            ch.sends +%= 1;
            atomicAdd(&total_sends, 1);
            result = @intCast(target.request_len);
        },
        .drop_too_small => {
            ch.lifetime.recv_buffer_small +%= 1;
            resetSlotLocked(slot);
            release_permit = true;
        },
        .deliver => {
            const response = out.?;
            const len: usize = @intCast(slot.meta.len);
            if (len != 0) {
                @memcpy(response[0..len], slot.data[0..len]);
                ch.lifetime.payload_copy_bytes +%= len;
            }
            ch.sends +%= 1;
            ch.receives +%= 1;
            atomicAdd(&total_sends, 1);
            atomicAdd(&total_receives, 1);
            result = @intCast(len);
            resetSlotLocked(slot);
            release_permit = true;
        },
        .none => ch.lifetime.stale_drops +%= 1,
    }
    unlockChannel(&guard);
    if (release_permit) _ = ch.slots_available.release(1);
    if (result < 0) atomicAdd(&total_errors, 1);
    return result;
}

fn publishReady(channel_id: u32, payload: []const u8, state: queue_model.State, count_call: bool, count_send: bool) i32 {
    const idx = index(channel_id) orelse return fail();
    var guard = lockChannel(&channels[idx]) orelse return fail();
    const ch = guard.channel;
    if (!ch.active) activateLocked(ch, channel_id);
    if (count_call) ch.lifetime.send_calls +%= 1;
    if (!ch.slots_available.tryAcquire()) {
        ch.lifetime.queue_full +%= 1;
        ch.drops +%= 1;
        atomicAdd(&total_drops, 1);
        unlockChannel(&guard);
        return fail();
    }
    const selection = queue_model.firstFree(&ch.queue) orelse {
        unlockChannel(&guard);
        _ = ch.slots_available.release(1);
        return fail();
    };
    const slot = &ch.queue[selection.index];
    prepareSlotLocked(ch, slot, .raw, payload.len);
    if (payload.len != 0) {
        @memcpy(slot.data[0..payload.len], payload);
        ch.lifetime.payload_copy_bytes +%= payload.len;
    }
    ch.lifetime.request_bytes +%= payload.len;
    slot.meta.state = state;
    if (count_send) {
        ch.sends +%= 1;
        atomicAdd(&total_sends, 1);
    }
    unlockChannel(&guard);
    return @intCast(payload.len);
}

fn directSendFromWorker(channel_id: u32, payload: []const u8) i32 {
    var response: [MAX_MESSAGE_SIZE]u8 = undefined;
    const produced = invokeHandlerDirect(channel_id, payload, response[0..], false);
    if (produced < 0) return produced;
    const len: usize = @intCast(produced);
    const queued = publishReady(channel_id, response[0..len], .handler_ready, true, true);
    return if (queued < 0) queued else @intCast(payload.len);
}

fn directRequestFromWorker(channel_id: u32, payload: []const u8, out: []u8) i32 {
    var response: [MAX_MESSAGE_SIZE]u8 = undefined;
    const produced = invokeHandlerDirect(channel_id, payload, response[0..], true);
    if (produced < 0) return produced;
    const len: usize = @intCast(produced);
    if (out.len < len) return fail();
    if (len != 0) @memcpy(out[0..len], response[0..len]);

    const idx = index(channel_id) orelse return fail();
    var guard = lockChannel(&channels[idx]) orelse return fail();
    const ch = guard.channel;
    ch.lifetime.payload_copy_bytes +%= len;
    ch.sends +%= 1;
    ch.receives +%= 1;
    atomicAdd(&total_sends, 1);
    atomicAdd(&total_receives, 1);
    unlockChannel(&guard);
    return produced;
}

fn invokeHandlerDirect(channel_id: u32, payload: []const u8, response: []u8, count_request: bool) i32 {
    const idx = index(channel_id) orelse return fail();
    var guard = lockChannel(&channels[idx]) orelse return fail();
    const ch = guard.channel;
    const handler = ch.handler orelse {
        unlockChannel(&guard);
        return fail();
    };
    const generation = ch.generation;
    ch.lifetime.handler_direct +%= 1;
    ch.lifetime.handler_started +%= 1;
    ch.lifetime.request_bytes +%= payload.len;
    if (count_request) ch.lifetime.request_calls +%= 1;
    unlockChannel(&guard);

    const started = monotonic.capture();
    const produced = handler(channel_id, payload, response);
    const completed = monotonic.capture();

    guard = lockChannel(&channels[idx]) orelse return fail();
    const result_ch = guard.channel;
    result_ch.lifetime.handler_completed +%= 1;
    noteDuration(&result_ch.lifetime, started, completed, .run);
    noteDuration(&result_ch.lifetime, started, completed, .e2e);
    const valid = result_ch.generation == generation and produced >= 0 and produced <= @as(i32, @intCast(response.len));
    if (valid) {
        result_ch.lifetime.response_bytes +%= @as(u64, @intCast(produced));
    } else {
        result_ch.lifetime.handler_failures +%= 1;
    }
    unlockChannel(&guard);
    return if (valid) produced else fail();
}

fn workerMain() callconv(.c) void {
    while (true) {
        if (takeNextWorkerTarget()) |target| {
            runWorkerTarget(target);
            continue;
        }
        atomicAdd(&worker_idle_waits, 1);
        const result = worker_event.waitResult(WAIT_FOREVER);
        if (result == .signaled) atomicAdd(&worker_wakes, 1);
    }
}

fn takeNextWorkerTarget() ?WorkerTarget {
    var offset: usize = 0;
    while (offset < channels.len) : (offset += 1) {
        const idx = (worker_cursor + offset) % channels.len;
        var guard = lockChannel(&channels[idx]) orelse continue;
        const ch = guard.channel;
        const selection = queue_model.oldestQueued(&ch.queue) orelse {
            unlockChannel(&guard);
            continue;
        };
        const slot = &ch.queue[selection.index];
        const handler = slot.handler orelse {
            slot.meta.result = -1;
            slot.meta.state = .handler_done;
            const completion = &slot.completion;
            unlockChannel(&guard);
            completion.complete();
            continue;
        };
        slot.meta.state = .handler_running;
        slot.started_at = monotonic.capture();
        ch.lifetime.handler_started +%= 1;
        noteDuration(&ch.lifetime, slot.submitted_at, slot.started_at, .queue);
        worker_cursor = (idx + 1) % channels.len;
        const target = WorkerTarget{
            .channel_index = idx,
            .channel_generation = slot.channel_generation,
            .slot_index = selection.index,
            .slot_generation = slot.meta.generation,
            .handler = handler,
        };
        unlockChannel(&guard);
        return target;
    }
    return null;
}

fn runWorkerTarget(target: WorkerTarget) void {
    const message = &channels[target.channel_index].queue[target.slot_index];
    const request_len: usize = @intCast(message.meta.len);
    var response: [MAX_MESSAGE_SIZE]u8 = undefined;
    const produced = target.handler(
        channels[target.channel_index].id,
        message.data[0..request_len],
        response[0..],
    );
    const completed_at = monotonic.capture();

    var guard = lockChannel(&channels[target.channel_index]) orelse return;
    const ch = guard.channel;
    const slot = &ch.queue[target.slot_index];
    if (!queue_model.matches(slot.meta, target.slot_generation, .handler_running)) {
        ch.lifetime.stale_drops +%= 1;
        unlockChannel(&guard);
        return;
    }

    ch.lifetime.handler_completed +%= 1;
    noteDuration(&ch.lifetime, slot.started_at, completed_at, .run);
    noteDuration(&ch.lifetime, slot.submitted_at, completed_at, .e2e);
    const valid = queue_model.channelGenerationMatches(ch.generation, target.channel_generation) and
        produced >= 0 and produced <= @as(i32, @intCast(MAX_MESSAGE_SIZE));
    slot.meta.result = if (valid) produced else -1;
    if (valid) {
        const len: usize = @intCast(produced);
        if (len != 0) {
            @memcpy(slot.data[0..len], response[0..len]);
            ch.lifetime.payload_copy_bytes +%= len;
        }
        slot.meta.len = @intCast(len);
        ch.lifetime.response_bytes +%= len;
    } else {
        slot.meta.len = 0;
        ch.lifetime.handler_failures +%= 1;
    }

    if (slot.meta.abandoned) {
        resetSlotLocked(slot);
        ch.lifetime.stale_drops +%= 1;
        unlockChannel(&guard);
        _ = ch.slots_available.release(1);
        return;
    }
    slot.meta.state = .handler_done;
    const completion = &slot.completion;
    unlockChannel(&guard);
    completion.complete();
    publishWorkerCompletion(target);
}

fn publishWorkerCompletion(target: WorkerTarget) void {
    var guard = lockChannel(&channels[target.channel_index]) orelse return;
    const ch = guard.channel;
    const slot = &ch.queue[target.slot_index];
    if (!queue_model.matches(slot.meta, target.slot_generation, .handler_done)) {
        unlockChannel(&guard);
        return;
    }
    slot.meta.completion_published = true;
    const abandoned = slot.meta.abandoned;
    if (abandoned) {
        resetSlotLocked(slot);
        ch.lifetime.stale_drops +%= 1;
    }
    unlockChannel(&guard);
    if (abandoned) _ = ch.slots_available.release(1);
}

const TimingKind = enum {
    queue,
    run,
    e2e,
};

fn noteDuration(lifetime: *Lifetime, start: monotonic.Stamp, end: monotonic.Stamp, kind: TimingKind) void {
    const elapsed = monotonic.elapsedNanoseconds(start, end) orelse {
        lifetime.timing_unavailable +%= 1;
        return;
    };
    switch (kind) {
        .queue => {
            lifetime.handler_queue_ns +%= elapsed;
            if (elapsed > lifetime.handler_queue_max_ns) lifetime.handler_queue_max_ns = elapsed;
        },
        .run => {
            lifetime.handler_run_ns +%= elapsed;
            if (elapsed > lifetime.handler_run_max_ns) lifetime.handler_run_max_ns = elapsed;
        },
        .e2e => {
            lifetime.handler_e2e_ns +%= elapsed;
            if (elapsed > lifetime.handler_e2e_max_ns) lifetime.handler_e2e_max_ns = elapsed;
        },
    }
}

fn prepareSlotLocked(ch: *Channel, slot: *Message, mode: queue_model.Mode, len: usize) void {
    const generation = nextNonZero(slot.meta.generation);
    const sequence = ch.next_sequence;
    ch.next_sequence = nextNonZero(ch.next_sequence);
    slot.meta = .{
        .state = .free,
        .mode = mode,
        .generation = generation,
        .sequence = sequence,
        .len = @intCast(len),
    };
    slot.channel_generation = ch.generation;
    slot.handler = null;
    slot.completion = sync.Completion.init();
    slot.submitted_at = .{};
    slot.started_at = .{};
}

fn resetSlotLocked(slot: *Message) void {
    const generation = slot.meta.generation;
    slot.meta = .{ .generation = generation };
    slot.channel_generation = 0;
    slot.handler = null;
    slot.submitted_at = .{};
    slot.started_at = .{};
}

fn activateLocked(ch: *Channel, channel_id: u32) void {
    ch.active = true;
    ch.id = channel_id;
    ch.generation = nextNonZero(ch.generation);
}

fn lockChannel(ch: *Channel) ?ChannelGuard {
    if (comptime builtin.is_test) return .{ .channel = ch, .admitted = true };
    if (scheduler.current() == null) return .{ .channel = ch, .admitted = true };
    const immediate = ch.lock.tryLock();
    if (!immediate and !ch.lock.lock(WAIT_FOREVER)) return null;
    if (!immediate) ch.lifetime.lock_contentions +%= 1;
    return .{ .channel = ch, .locked = true, .admitted = true };
}

fn unlockChannel(guard: *ChannelGuard) void {
    if (guard.locked) _ = guard.channel.lock.unlock();
    guard.locked = false;
    guard.admitted = false;
}

fn releaseChannelForWait(raw: *anyopaque) void {
    const guard: *ChannelGuard = @ptrCast(@alignCast(raw));
    unlockChannel(guard);
}

fn remainingTicks(deadline: u64, forever: bool) u64 {
    if (forever) return WAIT_FOREVER;
    const now = timer.tickCount();
    return if (now >= deadline) 0 else deadline - now;
}

fn currentIsWorker() bool {
    const current = scheduler.current() orelse return false;
    return worker_started and current.id == worker_task_id;
}

fn denyIrq() bool {
    if (!irq_router.inDispatch()) return false;
    atomicAdd(&irq_denied, 1);
    return true;
}

fn addLifetime(out: *PerformanceSummary, value: Lifetime) void {
    inline for (@typeInfo(Lifetime).@"struct".fields) |field| {
        @field(out, field.name) +%= @field(value, field.name);
    }
}

fn copyFixed(out: []u8, text: []const u8) void {
    @memset(out, 0);
    const len = if (text.len < out.len - 1) text.len else out.len - 1;
    if (len != 0) @memcpy(out[0..len], text[0..len]);
}

fn fixedText(bytes: []const u8) []const u8 {
    var len: usize = 0;
    while (len < bytes.len and bytes[len] != 0) : (len += 1) {}
    return bytes[0..len];
}

fn index(channel_id: u32) ?usize {
    if (channel_id == 0 or channel_id > MAX_CHANNELS) return null;
    return @intCast(channel_id - 1);
}

fn nextNonZero(value: u64) u64 {
    const next = value +% 1;
    return if (next == 0) 1 else next;
}

fn fail() i32 {
    atomicAdd(&total_errors, 1);
    return -1;
}

fn atomicAdd(counter: *u64, value: u64) void {
    _ = @atomicRmw(u64, counter, .Add, value, .monotonic);
}

fn atomicLoad(counter: *const u64) u64 {
    return @atomicLoad(u64, counter, .monotonic);
}

fn memEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}
