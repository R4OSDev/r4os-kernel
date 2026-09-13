//! Common cursor transport. GPU policy stays in the R4D, software rendering
//! in Desktop. The driver worker consumes one bounded job; no new thread.
const std = @import("std");
const model = @import("cursor_state.zig");
pub const a = model.a;
const sources = @import("cursor_source.zig");
const ownership = @import("ownership.zig");
const display = @import("display.zig");
const native = @import("native_driver.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const buffer_api = @import("../program/gfx_buffer_api.zig");
const queue = @import("queue.zig");
const clock = @import("../platform/monotonic.zig");
const events = @import("../kernel/desktop_events.zig");
const irq = @import("../kernel/irq_router.zig");
var state: model.State = .{};
var source: sources.Source = .{};
var admission = ownership.Execution.init("display-cursor");
pub fn code(err: anyerror) i32 {
    return switch (err) {
        error.Busy => a.gfx_output_error_busy,
        error.Stale, error.WrongOwner, error.Closed => a.gfx_output_error_stale,
        error.Unsupported, error.Disabled => a.gfx_output_error_unsupported,
        error.OutOfMemory, error.Capacity, error.Budget, error.Exhausted, error.Overflow => a.gfx_output_error_capacity,
        error.Unavailable => a.gfx_output_error_unavailable,
        else => a.gfx_output_error_invalid,
    };
}
fn nowNs() u64 { return clock.nowNanoseconds() orelse 0; }
fn snapshot() model.State {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    return state;
}
fn wake() void {
    const current = snapshot();
    if (current.driver.valid() and !current.lost()) queue.wakeNative(@intCast(current.driver.id), .{
        .adapter = current.info.backend.adapter_id, .device_generation = current.info.backend.device_generation,
        .reset_generation = current.info.backend.reset_generation }) catch {};
    events.signal();
}
fn releaseSource() !void {
    buffers.lock();
    source.close(&buffers.store) catch |err| { buffers.unlock(); return err; };
    buffers.unlock();
    buffers.collect();
}
fn lost(error_code: i32) void {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    state.markLost(error_code);
}
// Admission serializes resource changes. Stop only marks metadata and wakes;
// it never races BO release or requires the dying program to run again.
fn maintenance(now: u64) !void {
    const current = snapshot();
    const job = current.job orelse return;
    if (!current.taken and ((current.closing and job.request.operation != a.display_cursor_operation_release) or now >= job.deadline_ns)) {
        releaseSource() catch |err| { lost(code(err)); return err; };
        const token = ownership.enterState();
        state.rejectUntaken(if (current.closing) a.gfx_output_error_stale else a.gfx_output_error_timeout);
        ownership.leaveState(token);
        wake();
    } else if (current.taken and now >= job.deadline_ns and !current.lost()) {
        lost(a.gfx_output_error_timeout);
        events.signal(); // Keep source and unknown plane ownership until a real receipt.
    }
}
pub fn configure(driver: buffers.Owner, input: *const a.DisplayCursorInfo) i32 {
    if (irq.inDispatch() or @intFromPtr(input) == 0) return a.gfx_output_error_invalid;
    if (!display.beginOutputCommit()) return a.gfx_output_error_busy;
    defer display.endOutputCommit();
    if (!admission.tryEnter()) return a.gfx_output_error_busy;
    defer admission.leave();
    const target = native.cursorBinding() catch |err| return code(err);
    if (!target.driver.eql(driver) or input.display_generation != target.generation or input.head_id != target.head or
        !std.meta.eql(input.backend, target.backend)) return a.gfx_output_error_stale;
    {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        state.configure(driver, input.*) catch |err| return code(err);
    }
    events.signal();
    return a.gfx_output_ok;
}
pub fn info(output: *a.DisplayCursorInfo) callconv(.c) i32 {
    if (!buffer_api.validOutput(a.DisplayCursorInfo, output) or irq.inDispatch()) return a.gfx_output_error_invalid;
    const current = snapshot();
    if (!current.available()) return a.gfx_output_error_unsupported;
    const active = display.backendState();
    if (active.state != .software_native or active.owner != current.driver.id or active.generation != current.info.display_generation)
        return a.gfx_output_error_unsupported;
    output.* = current.info; return a.gfx_output_ok;
}
pub fn status(output: *a.DisplayCursorStatus) callconv(.c) i32 {
    if (!buffer_api.validOutput(a.DisplayCursorStatus, output) or irq.inDispatch()) return a.gfx_output_error_invalid;
    if (!admission.tryEnter()) return a.gfx_output_error_busy;
    defer admission.leave();
    maintenance(nowNs()) catch {};
    const current = snapshot();
    if (current.info.display_generation == 0) return a.gfx_output_error_unsupported;
    output.* = current.status; return a.gfx_output_ok;
}
pub fn submit(caller: buffers.Owner, input: *const a.DisplayCursorRequest, output: *a.DisplayCursorStatus) i32 {
    if (irq.inDispatch() or @intFromPtr(input) == 0 or !buffer_api.validOutput(a.DisplayCursorStatus, output)) return a.gfx_output_error_invalid;
    const call = ownership.retainCall();
    if (!call.admitted()) return a.gfx_output_error_busy;
    defer ownership.releaseCall(call);
    if (!display.beginOutputCommit()) return a.gfx_output_error_busy;
    defer display.endOutputCommit();
    if (!admission.tryEnter()) return a.gfx_output_error_busy;
    defer admission.leave();
    const now = nowNs();
    maintenance(now) catch |err| return code(err);
    const target = native.cursorBinding() catch |err| return code(err);
    const current = snapshot();
    if (!target.driver.eql(current.driver) or target.generation != current.info.display_generation or
        !std.meta.eql(target.backend, current.info.backend)) return a.gfx_output_error_stale;
    current.validate(caller, input.*) catch |err| return code(err);
    if (input.operation == a.display_cursor_operation_prepare) {
        buffers.lock();
        source.open(&buffers.store, caller, target.driver, input.*) catch |err| { buffers.unlock(); return code(err); };
        buffers.unlock();
    }
    const result = blk: {
        const token = ownership.enterState();
        state.begin(caller, input.*, source.wire(), target.timeline, target.point, now) catch |err| {
            ownership.leaveState(token);
            releaseSource() catch |release| lost(code(release));
            return code(err);
        };
        const accepted = state.status;
        ownership.leaveState(token);
        break :blk accepted;
    };
    output.* = result;
    wake(); return a.gfx_output_ok;
}
pub fn take(driver: buffers.Owner, backend: *const a.GfxBackendBinding, output: *a.GfxDriverCursorJob) i32 {
    if (irq.inDispatch() or @intFromPtr(backend) == 0 or !buffer_api.validOutput(a.GfxDriverCursorJob, output)) return a.gfx_output_error_invalid;
    if (!admission.tryEnter()) return a.gfx_output_error_busy;
    defer admission.leave();
    queue.validateOutputBinding(@intCast(driver.id), backend.*) catch |err| return code(err);
    const now = nowNs();
    maintenance(now) catch |err| return code(err);
    const job = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        break :blk state.take(driver, backend.*, now) catch |err| return code(err);
    };
    if (job) |value| { output.* = value; events.signal(); return a.gfx_output_ok; }
    return 0;
}
pub fn complete(driver: buffers.Owner, input: *const a.GfxDriverCursorCompletion) i32 {
    if (irq.inDispatch() or @intFromPtr(input) == 0) return a.gfx_output_error_invalid;
    if (!admission.tryEnter()) return a.gfx_output_error_busy;
    defer admission.leave();
    const current = snapshot();
    const changed = current.validateReply(driver, input.*) catch |err| return code(err);
    if (!changed) return a.gfx_output_ok;
    if (input.outcome != a.gfx_output_outcome_lost) {
        queue.validateOutputBinding(@intCast(driver.id), current.info.backend) catch |err| return code(err);
        releaseSource() catch |err| { lost(code(err)); return code(err); };
    }
    {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        state.finish(input.*);
    }
    wake(); return a.gfx_output_ok;
}
pub fn stopped(caller: buffers.Owner) void {
    const changed = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        break :blk state.close(caller);
    };
    if (changed) wake();
}
pub fn stoppedDriver(id: u32) void {
    const changed = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (id == 0 or state.driver.id != id or state.info.display_generation == 0 or state.lost()) break :blk false;
        state.markLost(a.gfx_output_error_unavailable); break :blk true;
    };
    if (changed) events.signal();
}
// The mode worker holds DisplayExecution, so public cursor submissions are
// excluded. Its job is exposed only after a confirmed hidden cursor plane.
pub fn beforeMode(driver: buffers.Owner) !void {
    if (!admission.tryEnter()) return error.Busy;
    defer admission.leave();
    maintenance(nowNs()) catch |err| return err;
    const ready = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (state.info.flags == 0) break :blk true;
        if (!state.driver.eql(driver) or state.lost()) return error.Unavailable;
        state.suspended = true;
        state.status.flags |= a.display_cursor_state_suspended;
        break :blk state.job == null and !state.visible();
    };
    if (!ready) { wake(); return error.Busy; }
}
pub fn resumeModes() void {
    // The new image can be shown during confirmation while its previous
    // backing remains retained. Keep the cursor in software for that whole
    // transaction; public cursor admission also excludes the replacement.
    if (native.modePending()) return;
    const token = ownership.enterState();
    state.suspended = false;
    state.status.flags &= ~a.display_cursor_state_suspended;
    ownership.leaveState(token);
    events.signal();
}
