//! Additional output routing. The existing BO/queue mutex owns these bounded
//! lifetimes, followed by the output state owner for catalog mutation. No
//! framebuffer work, driver callback, wait or allocation runs under either.
const std = @import("std");
const state = @import("output_runtime_state.zig");
const abi = state.abi;
const targets = @import("output_target.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const queue = @import("queue.zig");
const resources = @import("queue_resources.zig");
const outputs = @import("outputs.zig");
const ownership = @import("ownership.zig");
const events = @import("../kernel/desktop_events.zig");
pub const Error = outputs.Error || state.Error;
var store: state.Store = .{};
var admission = @import("../sched/sync.zig").UnwindGuard.init("additional-output-admission");
pub const ModeBinding = struct {
    driver: buffers.Owner,
    backend: abi.GfxBackendBinding,
    target: abi.GfxOutputTarget,
    width: u32,
    height: u32,
    active: bool,
};

fn port(target: abi.GfxOutputTarget) abi.GfxOutputId {
    return .{ .adapter_id = target.adapter_id, .connector_id = target.connector_id,
        .device_generation = target.device_generation, .connection_generation = target.connection_generation };
}
fn binding(value: abi.GfxBackendBinding) queue.model.Binding {
    return .{ .adapter = value.adapter_id, .device_generation = value.device_generation, .reset_generation = value.reset_generation };
}
pub fn register(driver: buffers.Owner, request: abi.GfxAdditionalOutput) Error!abi.GfxOutputTarget {
    if (!admission.enter(0)) return error.Busy;
    defer _ = admission.leave();
    buffers.lock(); defer buffers.unlock();
    try queue.requireOutputTargetsLocked(driver, binding(request.backend), request.job_size);
    for (&store.entries) |*entry| if (entry.driver.id != 0 and entry.target.adapter_id == request.backend.adapter_id and
        (entry.target.head_id == request.head_id or entry.target.connector_id == request.output.connector_id)) {
        if (queue.outputBusyLocked(entry.target)) return error.Busy;
    };
    const token = ownership.enterState(); defer ownership.leaveState(token);
    try outputs.validateAdditionalLocked(@intCast(driver.id), request);
    const target = try store.register(driver, request);
    _ = try outputs.pauseAdditionalLocked(@intCast(driver.id), request.output, true);
    return target;
}
pub fn transition(driver: buffers.Owner, target: abi.GfxOutputTarget, operation: u32, quiesced: u32) Error!void {
    if (operation > 2 or quiesced != @intFromBool(operation == 2)) return error.Invalid;
    if (!admission.enter(0)) return error.Busy;
    defer _ = admission.leave();
    if (operation == 2) {
        {
            buffers.lock(); defer buffers.unlock();
            const entry = try store.find(target);
            if (!entry.driver.eql(driver)) return error.WrongOwner;
            if (queue.outputBusyLocked(target)) return error.Busy;
            entry.removing = true;
        }
        // BO release may enter memory owners. No output/queue lock spans it.
        // The driver's quiescence proof applies only to this exact consumer.
        @import("native_additional_mode.zig").retire(driver, target) catch |err| {
            buffers.lock(); defer buffers.unlock();
            if (store.find(target)) |entry| {
                if (entry.driver.eql(driver)) entry.removing = false;
            } else |_| {}
            // A pending mode receipt must still be able to settle before
            // the driver retries physical removal of this output.
            return err;
        };
    }
    const changed = blk: {
        buffers.lock(); defer buffers.unlock();
        const entry = try store.find(target);
        if (!entry.driver.eql(driver)) return error.WrongOwner;
        if (operation == 0 and (entry.removing or entry.mode_blocked or entry.mode_lost)) return error.Busy;
        if (operation == 2 and queue.outputBusyLocked(target)) return error.Busy;
        const token = ownership.enterState(); defer ownership.leaveState(token);
        // Removal still succeeds after a disconnect replaced the receiver
        // identity; it cannot activate or otherwise affect that new receiver.
        const selected = outputs.currentIdentityLocked(@intCast(driver.id), port(target)) orelse port(target);
        if (operation == 0 and !std.meta.eql(selected, port(target))) return error.Stale;
        var paused_changed = false;
        if (operation == 0) {
            try outputs.validateAdditionalActivationLocked(@intCast(driver.id), selected, target.head_id, entry.width, entry.height);
            paused_changed = try outputs.pauseAdditionalLocked(@intCast(driver.id), selected, false);
        } else {
            paused_changed = outputs.pauseAdditionalLocked(@intCast(driver.id), selected, true) catch |err| absent: {
                if (err != error.Stale) return err;
                // Withdrawal already replaced this exact catalog identity.
                break :absent false;
            };
        }
        const changed = outputs.setNativeActiveLocked(@intCast(driver.id), selected, target.head_id, operation == 0) catch |err| {
            if (operation == 0) return err;
            // A withdrawn catalog entry has already lost its active flag.
            if (err != error.Stale) return err;
            outputs.forgetNativePortLocked(@intCast(driver.id), selected);
            entry.active = false;
            if (operation == 2) entry.* = .{};
            break :blk true;
        };
        entry.active = operation == 0;
        if (operation == 2) entry.* = .{};
        break :blk changed or paused_changed;
    };
    if (operation == 1) _ = @import("mode_work.zig").withdrawingOutput(@intCast(driver.id), port(target));
    if (changed) events.signal();
}
pub fn targetAt(adapter: u32, head: u32) Error!?abi.GfxOutputTarget {
    buffers.lock(); defer buffers.unlock();
    const entry = store.at(adapter, head) orelse return null;
    const current = try outputs.activeTarget(@intCast(entry.driver.id), port(entry.target), entry.target.display_generation);
    if (!targets.same(current, entry.target)) return error.Stale;
    return current;
}
pub fn contains(target: abi.GfxOutputTarget) bool {
    buffers.lock(); defer buffers.unlock();
    _ = store.find(target) catch return false;
    return true;
}
pub fn submitImage(caller: buffers.Owner, timeline: u64, submission: queue.model.Submission, request: resources.Request) queue.Error!queue.model.Status {
    const result = blk: {
        buffers.lock(); defer buffers.unlock();
        const entry = store.find(request.display_target) catch return error.Stale;
        if (!entry.active or entry.mode_lost or entry.removing) return error.Unavailable;
        if (entry.mode_blocked) return error.Busy;
        if (entry.pending) |fence| {
            const status = queue.queryLocked(fence) catch null;
            if (status) |value| if (value.phase != .terminal or (!entry.direct and (value.device_active or value.resources_held))) return error.Busy;
            entry.pending = null;
        }
        const descriptor = try buffers.store.describe(request.source, caller);
        const presentation = try entry.info();
        if ((request.operation != .present and request.operation != .direct_present) or descriptor.width != entry.width or descriptor.height != entry.height or
            @intFromEnum(descriptor.format) != presentation.format or descriptor.plane_count != 1 or descriptor.planes[0].offset != 0) return error.Unsupported;
        const accepted = try queue.submitOutputLocked(caller, timeline, submission, request, binding(entry.binding));
        entry.pending = accepted.fence;
        entry.direct = request.operation == .direct_present;
        break :blk accepted;
    };
    queue.wake();
    return result;
}
pub fn info(target: abi.GfxOutputTarget) Error!abi.DisplayPresentationInfo {
    buffers.lock(); defer buffers.unlock();
    const entry = try store.find(target);
    if (entry.active) {
        const current = try outputs.activeTarget(@intCast(entry.driver.id), port(target), target.display_generation);
        if (!targets.same(current, target)) return error.Stale;
    }
    return entry.info();
}
pub fn feedback(target: abi.GfxOutputTarget, source: abi.GfxFence) Error!abi.DisplayPresentationStats {
    buffers.lock(); defer buffers.unlock();
    const entry = try store.find(target);
    var value = try entry.statistics.feedback(target.head_id, source, entry.snapshot());
    if (entry.mode_lost) value.flags |= abi.display_presentation_flag_lost;
    return value;
}
pub fn modeBinding(assignment: abi.GfxScanoutState) Error!ModeBinding {
    buffers.lock(); defer buffers.unlock();
    for (&store.entries) |*entry| {
        if (entry.driver.id == 0 or !std.meta.eql(port(entry.target), assignment.output)) continue;
        if (entry.removing or entry.mode_lost or entry.target.head_id != assignment.head_id) return error.Stale;
        return .{ .driver = entry.driver, .backend = entry.binding, .target = entry.target,
            .width = entry.width, .height = entry.height, .active = entry.active };
    }
    return error.Stale;
}
pub fn beginMode(expected: ModeBinding) Error!void {
    buffers.lock(); defer buffers.unlock();
    const entry = try store.find(expected.target);
    if (!entry.driver.eql(expected.driver) or !std.meta.eql(entry.binding, expected.backend) or entry.removing) return error.Stale;
    if (queue.outputBusyLocked(entry.target)) return error.Busy;
    entry.pending = null;
    entry.mode_blocked = true;
}
pub fn finishMode(expected: ModeBinding, width: u32, height: u32, active: bool, lost: bool) Error!void {
    {
        buffers.lock(); defer buffers.unlock();
        const entry = try store.find(expected.target);
        if (!entry.driver.eql(expected.driver) or !std.meta.eql(entry.binding, expected.backend) or entry.removing or
            width == 0 or height == 0 or width > 65536 or height > 65536) return error.Stale;
        const token = ownership.enterState(); defer ownership.leaveState(token);
        _ = try outputs.pauseAdditionalLocked(@intCast(entry.driver.id), port(entry.target), !active or lost);
        _ = try outputs.setNativeActiveLocked(@intCast(entry.driver.id), port(entry.target), entry.target.head_id, active and !lost);
        entry.width = width; entry.height = height;
        entry.active = active and !lost;
        entry.mode_lost = lost;
        entry.mode_blocked = lost;
    }
    events.signal();
}
pub fn publishInfo(driver: buffers.Owner, value: abi.DisplayPresentationInfo) Error!bool {
    buffers.lock(); defer buffers.unlock();
    for (&store.entries) |*entry| if (entry.driver.eql(driver) and entry.target.display_generation == value.display_generation) {
        try entry.publishInfo(driver, value); return true;
    };
    return false;
}
pub fn publishStats(driver: buffers.Owner, value: abi.DisplayPresentationStats) Error!bool {
    buffers.lock(); defer buffers.unlock();
    for (&store.entries) |*entry| if (entry.driver.eql(driver) and entry.target.display_generation == value.display_generation) {
        try entry.publishStats(driver, value); return true;
    };
    return false;
}
pub fn stoppedDriver(driver: u32) void {
    buffers.lock(); defer buffers.unlock();
    store.stop(driver);
}
// Exact backend quiescence has already retired mode surfaces and queue jobs.
// Each additional target still passes its ordinary consumer-removal checks.
pub fn retireAfterReset(driver: buffers.Owner, backend: abi.GfxBackendBinding) Error!void {
    if (!admission.enter(0)) return error.Busy;
    defer _ = admission.leave();
    for (0..store.entries.len) |index| {
        const target = blk: {
            buffers.lock(); defer buffers.unlock();
            const entry = &store.entries[index];
            if (!entry.driver.eql(driver) or !std.meta.eql(entry.binding, backend)) continue;
            break :blk entry.target;
        };
        try transition(driver, target, 2, 1);
    }
}
