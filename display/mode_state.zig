//! Fixed private C transaction. Optional payloads have explicit presence.
comptime { if (@import("builtin").is_test) _ = @import("rust_owner_test"); }
const std = @import("std");
pub const abi = @import("r4os_kernel_contract");
pub const Owner = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Busy, Exhausted };
pub const operation_ns: u64 = 30 * std.time.ns_per_s;
const empty_owner: Owner = .{ .kind = .kernel, .id = 0, .generation = 0 };
pub const State = extern struct {
    status: abi.GfxModeStatus = .{}, job: abi.GfxDriverModeJob = .{},
    has_color: bool = false, color_data: abi.GfxDriverModeColor = .{}, caller: Owner = empty_owner, driver: Owner = empty_owner,
    confirmation_ns: u64 = 0, offered: bool = false, taken: bool = false, cancelled: bool = false, expired: bool = false,
    automatic: bool = false, has_reply: bool = false, reply_data: abi.GfxDriverModeCompletion = .{}, device_retired: bool = false,
    pub fn replyValue(self: *const State) ?abi.GfxDriverModeCompletion { return if (self.has_reply) self.reply_data else null; }
    pub fn available(self: *const State) bool { return r4os_rust_display_mode_available(self); }
    pub fn presentationReady(self: *const State, driver: Owner, backend: abi.GfxBackendBinding) bool { return r4os_rust_display_mode_presentation_ready(self, &driver, &backend); }
    pub fn retireAfterReset(self: *State, driver: Owner, backend: abi.GfxBackendBinding) Error!void { try status(r4os_rust_display_mode_retire(self, &driver, &backend)); }
    pub fn begin(self: *State, caller: Owner, driver: Owner, job: abi.GfxDriverModeJob, revision: u64, confirmation_ms: u32, now: u64) Error!void {
        return self.beginColor(caller, driver, job, null, revision, confirmation_ms, now);
    }
    pub fn beginColor(self: *State, caller: Owner, driver: Owner, job: abi.GfxDriverModeJob, color: ?abi.GfxDriverModeColor, revision: u64, confirmation_ms: u32, now: u64) Error!void {
        const value = color; try status(r4os_rust_display_mode_begin_color(self, &caller, &driver, &job, if (value) |*input| input else null, revision, confirmation_ms, now));
    }
    pub fn needsStart(self: *const State) bool { return r4os_rust_display_mode_needs_start(self); }
    pub fn arm(self: *State) Error!void { try status(r4os_rust_display_mode_arm(self)); }
    pub fn take(self: *State, driver: Owner, backend: abi.GfxBackendBinding) Error!?abi.GfxDriverModeJob {
        var out: abi.GfxDriverModeJob = undefined; var has: bool = undefined;
        try status(r4os_rust_display_mode_take(self, &driver, &backend, &out, &has)); return if (has) out else null;
    }
    pub fn complete(self: *State, driver: Owner, value: abi.GfxDriverModeCompletion) Error!void { try status(r4os_rust_display_mode_complete(self, &driver, &value)); }
    pub fn readColor(self: *const State, driver: Owner, ticket: u64, sequence: u64) Error!?abi.GfxDriverModeColor {
        var out: abi.GfxDriverModeColor = undefined; var has: bool = undefined;
        try status(r4os_rust_display_mode_read_color(self, &driver, ticket, sequence, &out, &has)); return if (has) out else null;
    }
    pub fn resolve(self: *State, caller: Owner, ticket: u64, action: u32, now: u64) Error!void { try status(r4os_rust_display_mode_resolve(self, &caller, ticket, action, now)); }
    pub fn deadline(self: *const State) u64 { return r4os_rust_display_mode_deadline(self); }
    pub fn outputGone(self: *State, driver_id: u32, output: abi.GfxOutputId, now: u64) bool { return r4os_rust_display_mode_output_gone(self, driver_id, &output, now); }
    pub fn expire(self: *State, now: u64) bool { return r4os_rust_display_mode_expire(self, now); }
    pub fn rejectUntaken(self: *State, error_code: i32) void { r4os_rust_display_mode_reject(self, error_code); }
    pub fn settled(self: *State, result: abi.GfxAtomicResult, now: u64) void { r4os_rust_display_mode_settled(self, &result, now); }
};
fn status(code: i32) Error!void { return switch (code) { 0 => {}, 1 => error.Invalid, 4 => error.Exhausted, 7 => error.Stale, 8 => error.Busy, else => unreachable }; }
extern fn r4os_rust_display_mode_available(*const State) bool;
extern fn r4os_rust_display_mode_presentation_ready(*const State, *const Owner, *const abi.GfxBackendBinding) bool;
extern fn r4os_rust_display_mode_retire(*State, *const Owner, *const abi.GfxBackendBinding) i32;
extern fn r4os_rust_display_mode_begin_color(*State, *const Owner, *const Owner, *const abi.GfxDriverModeJob, ?*const abi.GfxDriverModeColor, u64, u32, u64) i32;
extern fn r4os_rust_display_mode_needs_start(*const State) bool;
extern fn r4os_rust_display_mode_arm(*State) i32;
extern fn r4os_rust_display_mode_take(*State, *const Owner, *const abi.GfxBackendBinding, *abi.GfxDriverModeJob, *bool) i32;
extern fn r4os_rust_display_mode_complete(*State, *const Owner, *const abi.GfxDriverModeCompletion) i32;
extern fn r4os_rust_display_mode_read_color(*const State, *const Owner, u64, u64, *abi.GfxDriverModeColor, *bool) i32;
extern fn r4os_rust_display_mode_resolve(*State, *const Owner, u64, u32, u64) i32;
extern fn r4os_rust_display_mode_deadline(*const State) u64;
extern fn r4os_rust_display_mode_output_gone(*State, u32, *const abi.GfxOutputId, u64) bool;
extern fn r4os_rust_display_mode_expire(*State, u64) bool;
extern fn r4os_rust_display_mode_reject(*State, i32) void;
extern fn r4os_rust_display_mode_settled(*State, *const abi.GfxAtomicResult, u64) void;
comptime {
    if (@sizeOf(State) != 696 or @alignOf(State) != 8 or @offsetOf(State, "job") != 88 or @offsetOf(State, "color_data") != 432 or @offsetOf(State, "caller") != 584 or @offsetOf(State, "driver") != 608 or @offsetOf(State, "confirmation_ns") != 632 or @offsetOf(State, "reply_data") != 648 or @offsetOf(State, "device_retired") != 688) @compileError("Native mode transaction C layout");
}

fn checkColorTransaction() !void {
    const t = std.testing;
    const bo = @import("../memory/gfx_buffer_layout.zig");
    const check = @import("mode_color_state.zig").validate;
    const identity: abi.GfxOutputId = .{ .adapter_id = 5, .connector_id = 2, .device_generation = 6, .connection_generation = 7 };
    var source: abi.GfxOutputColorState = .{ .identity = identity, .revision = 8, .flags = 15,
        .formats = 3, .depths = 3, .color_spaces = 3, .transfers = 13, .ranges = 3 };
    var request: abi.GfxModeColorRequest = .{ .state = .{ .topology_revision = 8, .count = 1 },
        .image = .{ .id = 3, .generation = 4 }, .signal = .{ .format = abi.gfx_buffer_format_xrgb2101010,
            .bpc = 10, .primaries = 3, .transfer = 3, .range = 2, .pipeline = 7,
            .reference_white = 2030000, .peak = 10000000, .metadata_valid = 1 } };
    request.state.assignments[0] = .{ .output = identity, .source_width = 640, .source_height = 480 };
    var image: bo.Descriptor = .{ .bytes = 2560 * 480, .width = 640, .height = 480, .format = .xrgb2101010,
        .plane_count = 1, .planes = .{ .{ .pitch = 2560 }, .{}, .{}, .{} }, .usage = bo.Usage.cpu_write | bo.Usage.transfer_source };
    try check(&request, source, image);
    source.transfers = 1; try t.expectError(error.Unsupported, check(&request, source, image)); source.transfers = 13;
    source.flags = 7; try t.expectError(error.Unsupported, check(&request, source, image)); source.flags = 15;
    request.signal.pipeline = 3; try t.expectError(error.Unsupported, check(&request, source, image)); request.signal.pipeline = 7;
    request.signal.metadata_valid = 0; try t.expectError(error.Unsupported, check(&request, source, image)); request.signal.metadata_valid = 1;
    source.identity.connection_generation += 1; try t.expectError(error.Stale, check(&request, source, image)); source.identity = identity;
    image.format = .xrgb8888; try t.expectError(error.Unsupported, check(&request, source, image)); image.format = .xrgb2101010;
    image.planes[0].pitch = std.math.maxInt(u64); try t.expectError(error.Unsupported, check(&request, source, image));

    const caller: Owner = .{ .kind = .program, .id = 7, .generation = 9 };
    const driver: Owner = .{ .kind = .driver, .id = 12, .generation = 3 };
    const job: abi.GfxDriverModeJob = .{ .ticket = 4, .assignment = request.state.assignments[0],
        .backend = .{ .adapter_id = 5, .device_generation = 6, .reset_generation = 7 } };
    var extension: abi.GfxDriverModeColor = .{ .signal = request.signal, .reference = .{ .reference = .{ .id = 71, .generation = 17 } } };
    var state: State = .{};
    try state.beginColor(caller, driver, job, extension, 8, 1000, 100);
    extension.signal.peak = 1; extension.reference.reference.id = 0;
    try t.expectError(error.Stale, state.readColor(driver, 4, 1));
    try state.arm();
    const taken = (try state.take(driver, job.backend)).?;
    try t.expectError(error.Stale, state.readColor(caller, taken.ticket, taken.sequence));
    const copied = (try state.readColor(driver, taken.ticket, taken.sequence)).?;
    try t.expect(copied.ticket == 4 and copied.sequence == 1 and copied.signal.peak == 10000000 and copied.reference.reference.id == 71);
    try state.complete(driver, .{ .ticket = taken.ticket, .sequence = taken.sequence, .operation = taken.operation, .outcome = 1, .quiesced = 1 });
    try t.expectError(error.Stale, state.readColor(driver, taken.ticket, taken.sequence));
    state.settled(.{ .outcome = 1 }, 200);
    try t.expect(state.expire(state.status.confirmation_deadline_ns));
    try state.arm();
    const rollback = (try state.take(driver, job.backend)).?;
    try t.expectError(error.Stale, state.readColor(driver, taken.ticket, taken.sequence));
    const retained = (try state.readColor(driver, rollback.ticket, rollback.sequence)).?;
    try t.expect(std.meta.eql(retained.signal, copied.signal) and std.meta.eql(retained.reference, copied.reference) and retained.sequence == 2);
}

test "mode jobs bind owner generation and operation, preserve late receipts and automatically request rollback" {
    try checkColorTransaction();
    const t = std.testing;
    const caller: Owner = .{ .kind = .program, .id = 7, .generation = 9 };
    const driver: Owner = .{ .kind = .driver, .id = 12, .generation = 3 };
    var state: State = .{};
    const job: abi.GfxDriverModeJob = .{ .ticket = 4, .backend = .{ .adapter_id = 5, .device_generation = 6, .reset_generation = 7 } };
    try state.begin(caller, driver, job, 8, 15000, 100);
    try t.expect(!state.presentationReady(driver, job.backend));
    try t.expect((try state.take(driver, job.backend)) == null);
    try state.arm();
    var wrong = driver; wrong.generation += 1;
    try t.expectError(error.Stale, state.take(wrong, job.backend));
    const first = (try state.take(driver, job.backend)).?;
    try t.expect(!state.presentationReady(driver, job.backend));
    try t.expect((try state.take(driver, job.backend)) == null);
    var receipt: abi.GfxDriverModeCompletion = .{ .ticket = first.ticket, .sequence = first.sequence, .operation = first.operation,
        .outcome = abi.gfx_output_outcome_applied, .quiesced = 0 };
    try t.expectError(error.Invalid, state.complete(driver, receipt));
    receipt.quiesced = 1;
    try t.expect(state.expire(first.deadline_ns));
    try t.expect(state.status.phase == abi.gfx_mode_phase_lost and state.status.retained == 3 and !state.available());
    try t.expect(!state.presentationReady(driver, job.backend));
    try state.complete(driver, receipt);
    try t.expectError(error.Stale, state.complete(driver, receipt));
    state.settled(.{ .outcome = 1, .topology_revision = 9, .commit_sequence = 1 }, first.deadline_ns + 1);
    try t.expect(state.status.phase == abi.gfx_mode_phase_reverting and state.status.retained == 3);
    try state.arm();
    const rollback = (try state.take(driver, job.backend)).?;
    try t.expect(rollback.sequence > first.sequence and rollback.operation == abi.gfx_mode_operation_rollback);
    try t.expectError(error.Stale, state.complete(driver, receipt));
    receipt.sequence = rollback.sequence; receipt.operation = rollback.operation;
    try t.expectError(error.Invalid, state.complete(driver, receipt));
    receipt.outcome = 2; receipt.quiesced = 2;
    try state.complete(driver, receipt);
    state.settled(.{ .outcome = 2, .topology_revision = 10, .commit_sequence = 2 }, first.deadline_ns + 2);
    try t.expect(state.available() and state.status.retained == 1);

    var next = job; next.ticket += 1;
    try state.begin(caller, driver, next, 10, 1000, 100);
    try state.arm();
    const applied = (try state.take(driver, job.backend)).?;
    try state.complete(driver, .{ .ticket = applied.ticket, .sequence = applied.sequence, .operation = applied.operation, .outcome = 1, .quiesced = 1 });
    try t.expect(!state.presentationReady(driver, job.backend));
    state.settled(.{ .outcome = 1 }, 500);
    try t.expect(state.presentationReady(driver, job.backend) and state.status.retained == 3);
    try t.expect(!state.presentationReady(wrong, job.backend));
    var other_backend = job.backend;
    other_backend.reset_generation += 1;
    try t.expect(!state.presentationReady(driver, other_backend));
    var confirming_trial = state;
    try confirming_trial.resolve(caller, next.ticket, abi.gfx_mode_resolve_confirm, 501);
    try t.expect(!confirming_trial.presentationReady(driver, job.backend));
    const end = state.status.confirmation_deadline_ns;
    try t.expectError(error.Stale, state.resolve(driver, next.ticket, abi.gfx_mode_resolve_confirm, end - 1));
    try t.expectError(error.Busy, state.resolve(caller, next.ticket, abi.gfx_mode_resolve_confirm, end));
    try t.expect(state.expire(end) and state.status.phase == abi.gfx_mode_phase_reverting);
    try t.expect(!state.presentationReady(driver, job.backend));

    try t.expectError(error.Stale, state.retireAfterReset(wrong, job.backend));
    var new_backend = job.backend; new_backend.device_generation += 1;
    try t.expectError(error.Stale, state.retireAfterReset(driver, new_backend));
    try state.retireAfterReset(driver, job.backend);
    try t.expect(state.available() and state.status.phase == abi.gfx_mode_phase_lost and state.status.retained == 0);
    try t.expectError(error.Stale, state.complete(driver, receipt));
    var replacement = next; replacement.ticket += 1; replacement.backend = new_backend;
    try state.begin(caller, driver, replacement, 11, 1000, end + 1);
    try t.expect(!state.device_retired and !state.available());
    try t.expectError(error.Stale, state.retireAfterReset(driver, job.backend));

    // A rejected in-flight apply can also arrive after the output was hidden.
    // The common bridge must republish the retained old image for this result.
    var denied: State = .{};
    try denied.begin(caller, driver, next, 10, 1000, 100);
    try denied.arm();
    const attempt = (try denied.take(driver, job.backend)).?;
    try t.expect(denied.expire(attempt.deadline_ns));
    try denied.complete(driver, .{ .ticket = attempt.ticket, .sequence = attempt.sequence,
        .operation = attempt.operation, .outcome = abi.gfx_output_outcome_old_preserved, .quiesced = 2 });
    denied.settled(.{ .outcome = abi.gfx_output_outcome_old_preserved }, attempt.deadline_ns + 1);
    try t.expect(denied.status.phase == abi.gfx_mode_phase_reverted and denied.status.retained == 1);

    // HPD requests bounded cancellation but cannot forge a driver receipt.
    var unplug: State = .{};
    var connected = next;
    connected.assignment.output = .{ .adapter_id = 5, .device_generation = 6, .connector_id = 4, .connection_generation = 12 };
    try unplug.begin(caller, driver, connected, 10, 15000, 100);
    try unplug.arm();
    const in_flight = (try unplug.take(driver, connected.backend)).?;
    var foreign = connected.assignment.output; foreign.connection_generation += 1;
    try t.expect(!unplug.outputGone(driver.id + 1, connected.assignment.output, 110));
    try t.expect(!unplug.outputGone(driver.id, foreign, 110));
    try t.expect(unplug.outputGone(driver.id, connected.assignment.output, 110));
    try t.expect(!unplug.outputGone(driver.id, connected.assignment.output, 111));
    try t.expect(unplug.replyValue() == null and unplug.taken and unplug.status.retained == 3 and unplug.job.sequence == in_flight.sequence);
    try unplug.complete(driver, .{ .ticket = in_flight.ticket, .sequence = in_flight.sequence, .operation = in_flight.operation,
        .outcome = abi.gfx_output_outcome_applied, .quiesced = 1 });
    unplug.settled(.{ .outcome = abi.gfx_output_outcome_applied }, 120);
    try t.expect(unplug.status.phase == abi.gfx_mode_phase_reverting and unplug.job.sequence == in_flight.sequence + 1);

    var waiting: State = .{};
    try waiting.begin(caller, driver, connected, 10, 15000, 100);
    try waiting.arm();
    const applied_job = (try waiting.take(driver, connected.backend)).?;
    try waiting.complete(driver, .{ .ticket = applied_job.ticket, .sequence = applied_job.sequence, .operation = applied_job.operation,
        .outcome = abi.gfx_output_outcome_applied, .quiesced = 1 });
    waiting.settled(.{ .outcome = abi.gfx_output_outcome_applied }, 120);
    try t.expect(waiting.outputGone(driver.id, connected.assignment.output, 121));
    try t.expect(waiting.status.phase == abi.gfx_mode_phase_reverting and waiting.status.confirmation_deadline_ns == 0 and
        waiting.status.retained == 3 and waiting.replyValue() == null);

    var reconnect: State = .{};
    try reconnect.begin(driver, driver, connected, 10, 1000, 100);
    reconnect.automatic = true;
    try reconnect.arm();
    const restoring = (try reconnect.take(driver, connected.backend)).?;
    try reconnect.complete(driver, .{ .ticket = restoring.ticket, .sequence = restoring.sequence, .operation = restoring.operation,
        .outcome = abi.gfx_output_outcome_applied, .quiesced = 1 });
    reconnect.settled(.{ .outcome = abi.gfx_output_outcome_applied }, 120);
    try t.expect(reconnect.status.phase == abi.gfx_mode_phase_confirming and reconnect.status.confirmation_deadline_ns == 0 and
        reconnect.status.retained == 3 and reconnect.job.sequence == restoring.sequence + 1);
    try reconnect.arm();
    const confirming = (try reconnect.take(driver, connected.backend)).?;
    try reconnect.complete(driver, .{ .ticket = confirming.ticket, .sequence = confirming.sequence, .operation = confirming.operation,
        .outcome = abi.gfx_output_outcome_applied, .quiesced = 1 });
    reconnect.settled(.{ .outcome = abi.gfx_output_outcome_applied }, 130);
    try t.expect(reconnect.available() and reconnect.status.phase == abi.gfx_mode_phase_confirmed and reconnect.status.retained == 2);
}
