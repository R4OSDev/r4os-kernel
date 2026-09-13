// Common asynchronous mode owner. GPU allocation/commands stay in the R4D.
// This worker is separate from gfx-work: waiting here must not prevent that
// queue worker from notifying the very driver whose receipt is outstanding.
const std = @import("std");
const model = @import("mode_state.zig");
pub const abi = model.abi;
const ownership = @import("ownership.zig");
const display = @import("display.zig");
const native = @import("native_driver.zig");
const outputs = @import("outputs.zig");
const queue = @import("queue.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const sync = @import("../sched/sync.zig");
const scheduler = @import("../sched/scheduler.zig");
const task = @import("../sched/task.zig");
const clock = @import("../platform/monotonic.zig");
const timer = @import("../kernel/timer.zig");
const events = @import("../kernel/desktop_events.zig");
const irq = @import("../kernel/irq_router.zig");
pub const Error = outputs.Error || display.TransitionError;
pub const code = native.code;
var state: model.State = .{};
var binding: native.ModeBinding = undefined;
var worker_event = sync.Event.initMode(false, .auto_reset);
var admission = sync.UnwindGuard.init("display-mode-admission");
var started = false;

pub fn enable(driver: buffers.Owner, backend: abi.GfxBackendBinding) Error!void {
    if (irq.inDispatch()) return error.Invalid;
    if (!admission.enter(0)) return error.Busy;
    defer _ = admission.leave();
    if (!started) {
        _ = task.createKernelThreadWithRole("gfx-mode", workerMain, .short_completion) orelse return error.OutOfMemory;
        started = true;
    }
    try native.enableModes(driver, backend);
}
fn nowNs() u64 { return clock.nowNanoseconds() orelse 0; }
fn snapshot() model.State {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    return state;
}
pub fn status(ticket: u64) Error!abi.GfxModeStatus {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (ticket != 0 and ticket != state.status.ticket) return error.Stale;
    return state.status;
}
pub fn submit(caller: buffers.Owner, input: *const abi.GfxAtomicState, confirmation_ms: u32) Error!abi.GfxModeStatus {
    return submitImpl(caller, input, confirmation_ms, false);
}
pub fn restore(driver: buffers.Owner, input: *const abi.GfxAtomicState) Error!abi.GfxModeStatus {
    if (driver.kind != .driver or !driver.valid() or input.version != 1 or input.size < @sizeOf(abi.GfxAtomicState) or
        input.count != 1) return error.Invalid;
    if (!outputs.nativePaused(@intCast(driver.id), input.assignments[0].output)) return error.Stale;
    var state_copy = input.*;
    if (state_copy.topology_revision == 0) state_copy.topology_revision = outputs.revision().revision;
    return submitImpl(driver, &state_copy, 1000, true);
}
pub fn driverStatus(driver: buffers.Owner, ticket: u64) Error!abi.GfxModeStatus {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (ticket == 0 or ticket != state.status.ticket or !driver.eql(state.driver)) return error.Stale;
    return state.status;
}
fn submitImpl(caller: buffers.Owner, input: *const abi.GfxAtomicState, confirmation_ms: u32, automatic: bool) Error!abi.GfxModeStatus {
    if (irq.inDispatch() or confirmation_ms < 1000 or confirmation_ms > 60000) return error.Invalid;
    if (!admission.enter(0)) return error.Busy;
    defer _ = admission.leave();
    if (!started) return error.Unsupported;
    if (!snapshot().available()) return error.Busy;
    const now = nowNs();
    if (now == 0 or now > std.math.maxInt(u64) - model.operation_ns) return error.Unavailable;
    const mode = try outputs.nativeMode(caller, input);
    if (!display.beginOutputCommit()) return error.Busy;
    defer display.endOutputCommit();
    const target = try native.modeBinding(input.assignments[0]);
    if (!automatic and outputs.nativePaused(@intCast(target.driver.id), input.assignments[0].output)) return error.Busy;
    if (automatic and !target.driver.eql(caller)) return error.Stale;
    const reference = try native.prepareMode(caller, input.assignments[0], mode);
    errdefer native.abortPreparedMode();
    const accepted = try outputs.beginNative(caller, input);
    errdefer _ = outputs.finishNative(accepted.ticket, abi.gfx_mode_operation_apply, abi.gfx_output_outcome_old_preserved, 2) catch {};
    const result = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        try state.begin(caller, target.driver, .{ .ticket = accepted.ticket.id, .backend = target.backend,
            .assignment = input.assignments[0], .mode = accepted.mode, .reference = reference }, accepted.revision, confirmation_ms, now);
        state.automatic = automatic;
        binding = target;
        break :blk state.status;
    };
    worker_event.signal();
    events.signal();
    return result;
}
pub fn resolve(caller: buffers.Owner, ticket: u64, action: u32) Error!abi.GfxModeStatus {
    if (irq.inDispatch()) return error.Invalid;
    const now = nowNs();
    if (now == 0) return error.Unavailable;
    const result = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        try state.resolve(caller, ticket, action, now);
        break :blk state.status;
    };
    worker_event.signal();
    events.signal();
    return result;
}
pub fn take(driver: buffers.Owner, backend: abi.GfxBackendBinding) Error!?abi.GfxDriverModeJob {
    if (irq.inDispatch()) return error.Invalid;
    try queue.validateOutputBinding(@intCast(driver.id), backend);
    const result = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        // Idle workers may ask after every ordinary graphics notification.
        if (state.status.ticket == 0 or state.available()) break :blk null;
        break :blk try state.take(driver, backend);
    };
    if (result != null) events.signal();
    return result;
}
pub fn complete(driver: buffers.Owner, value: abi.GfxDriverModeCompletion) Error!void {
    if (irq.inDispatch()) return error.Invalid;
    const before = snapshot();
    if (value.outcome != abi.gfx_output_outcome_lost)
        try queue.validateOutputBinding(@intCast(driver.id), before.job.backend);
    {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        try state.complete(driver, value);
    }
    worker_event.signal();
}
pub fn stoppedDriver(id: u32) void {
    const changed = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (state.available() or state.driver.id != id) break :blk false;
        state.cancelled = true;
        state.expired = true;
        state.status.phase = abi.gfx_mode_phase_lost;
        state.status.outcome = abi.gfx_output_outcome_lost;
        state.status.error_code = abi.gfx_output_error_unavailable;
        state.status.operation_deadline_ns = 0;
        state.status.confirmation_deadline_ns = 0;
        // An actual outstanding receipt remains identifiable. Device removal
        // itself is not that receipt and cannot release either surface.
        break :blk true;
    };
    if (changed) worker_event.signal();
}
pub fn withdrawingOutput(driver_id: u32, output: abi.GfxOutputId) bool {
    const now = nowNs();
    var changed = false;
    const pending = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (state.available() or state.driver.kind != .driver or state.driver.id != driver_id or
            !std.meta.eql(state.job.assignment.output, output)) break :blk false;
        changed = state.outputGone(driver_id, output, now);
        break :blk true;
    };
    if (changed) { worker_event.signal(); events.signal(); }
    return pending;
}
fn rejected(error_code: i32) void {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (state.reply != null) return;
    if (!state.taken) state.rejectUntaken(error_code) else {
        state.cancelled = true;
        state.expired = true;
        state.status.phase = abi.gfx_mode_phase_lost;
        state.status.outcome = abi.gfx_output_outcome_lost;
        state.status.error_code = error_code;
        state.status.operation_deadline_ns = 0;
    }
}
fn ticksUntil(deadline: u64, now: u64) u64 {
    if (deadline == 0) return scheduler.WAIT_FOREVER;
    if (deadline <= now) return 1;
    return @max(1, @as(u64, @intCast((@as(u128, deadline - now) * @max(timer.frequency(), 1) + std.time.ns_per_s - 1) / std.time.ns_per_s)));
}
fn workerMain() callconv(.c) void {
    var locked = false;
    var hidden_ticket: u64 = 0;
    var hidden_sequence: u64 = 0;
    while (true) {
        const now = nowNs();
        const expired = blk: {
            const token = ownership.enterState(); defer ownership.leaveState(token);
            break :blk state.expire(now);
        };
        if (expired) events.signal();
        var current = snapshot();
        // Cancellation before take has no device effects. After take it waits
        // for the actual result, then restores instead of asking confirmation.
        if (current.cancelled and current.job.operation == abi.gfx_mode_operation_apply and !current.taken and
            current.reply == null and current.status.phase != abi.gfx_mode_phase_lost) {
            rejected(0);
            current = snapshot();
        }
        const needs_lock = current.needsStart() or current.reply != null or
            (current.status.phase == abi.gfx_mode_phase_lost and (hidden_ticket != current.status.ticket or hidden_sequence != current.job.sequence));
        if (needs_lock and !locked) {
            locked = display.beginOutputCommit();
            if (!locked) { _ = worker_event.waitResult(1); continue; }
        }
        if (current.needsStart()) {
            @import("cursor_work.zig").beforeMode(binding.driver) catch |err| {
                if (err == error.Busy) { _ = worker_event.waitResult(1); continue; }
                rejected(@import("cursor_work.zig").code(err));
                continue;
            };
            native.startMode(current.job.operation, binding) catch |err| {
                if (err == error.Busy) { _ = worker_event.waitResult(1); continue; }
                rejected(code(err));
                continue;
            };
            {
                const token = ownership.enterState(); defer ownership.leaveState(token);
                state.arm() catch {};
            }
            queue.wakeNative(@intCast(current.driver.id), .{ .adapter = current.job.backend.adapter_id,
                .device_generation = current.job.backend.device_generation, .reset_generation = current.job.backend.reset_generation }) catch |err| rejected(code(err));
            continue;
        }
        if (current.reply) |receipt| {
            var outcome = receipt.outcome;
            queue.validateOutputBinding(@intCast(current.driver.id), current.job.backend) catch {
                outcome = abi.gfx_output_outcome_lost;
            };
            outputs.canFinishNative(.{ .id = receipt.ticket }, receipt.operation) catch {
                outcome = abi.gfx_output_outcome_lost;
            };
            native.settleMode(receipt.operation, outcome) catch |err| {
                outcome = abi.gfx_output_outcome_lost;
                native.settleMode(receipt.operation, outcome) catch {};
                const token = ownership.enterState();
                state.status.error_code = code(err);
                ownership.leaveState(token);
            };
            const result = outputs.finishNative(.{ .id = receipt.ticket }, receipt.operation, outcome,
                if (outcome == abi.gfx_output_outcome_lost) 0 else receipt.quiesced) catch abi.GfxAtomicResult{ .outcome = abi.gfx_output_outcome_lost, .retained = 3 };
            {
                const token = ownership.enterState(); defer ownership.leaveState(token);
                state.settled(result, now);
            }
            events.signal();
            continue;
        }
        if (current.status.phase == abi.gfx_mode_phase_lost and (hidden_ticket != current.status.ticket or hidden_sequence != current.job.sequence)) {
            native.settleMode(current.job.operation, abi.gfx_output_outcome_lost) catch {};
            hidden_ticket = current.status.ticket;
            hidden_sequence = current.job.sequence;
            events.signal();
        }
        if (locked and (!current.offered or current.expired)) {
            @import("cursor_work.zig").resumeModes();
            display.endOutputCommit();
            locked = false;
        }
        _ = worker_event.waitResult(ticksUntil(current.deadline(), now));
    }
}
