// One bounded primary-output transaction. This module owns only copied
// metadata; the display bridge owns references and the R4D owns GPU state.
const std = @import("std");
pub const abi = @import("r4os_kernel_contract");
pub const Owner = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Busy, Exhausted };
pub const operation_ns: u64 = 30 * std.time.ns_per_s;
const empty_owner: Owner = .{ .kind = .kernel, .id = 0, .generation = 0 };

pub const State = struct {
    status: abi.GfxModeStatus = .{},
    job: abi.GfxDriverModeJob = .{},
    caller: Owner = empty_owner,
    driver: Owner = empty_owner,
    confirmation_ns: u64 = 0,
    offered: bool = false,
    taken: bool = false,
    cancelled: bool = false,
    expired: bool = false,
    reply: ?abi.GfxDriverModeCompletion = null,

    pub fn available(self: *const State) bool {
        return self.status.phase == abi.gfx_mode_phase_idle or self.status.phase == abi.gfx_mode_phase_confirmed or
            self.status.phase == abi.gfx_mode_phase_reverted;
    }
    pub fn begin(self: *State, caller: Owner, driver: Owner, job: abi.GfxDriverModeJob, revision: u64, confirmation_ms: u32, now: u64) Error!void {
        if (!self.available()) return error.Busy;
        if (!caller.valid() or driver.kind != .driver or !driver.valid() or job.ticket == 0 or job.ticket <= self.status.ticket or
            confirmation_ms < 1000 or confirmation_ms > 60000 or now == 0 or now > std.math.maxInt(u64) - operation_ns) return error.Invalid;
        self.* = .{ .caller = caller, .driver = driver, .job = job, .confirmation_ns = @as(u64, confirmation_ms) * std.time.ns_per_ms,
            .status = .{ .ticket = job.ticket, .topology_revision = revision, .output = job.assignment.output, .retained = 3 } };
        self.schedule(abi.gfx_mode_operation_apply, now);
    }
    fn schedule(self: *State, operation: u32, now: u64) void {
        // At most apply + confirm + rollback; a transaction never loops or
        // reuses an operation sequence after an uncertain hardware failure.
        self.job.sequence += 1;
        self.job.operation = operation;
        self.job.deadline_ns = now +| operation_ns;
        self.status.operation_deadline_ns = self.job.deadline_ns;
        self.status.confirmation_deadline_ns = 0;
        self.status.phase = switch (operation) {
            abi.gfx_mode_operation_apply => abi.gfx_mode_phase_queued,
            abi.gfx_mode_operation_confirm => abi.gfx_mode_phase_confirming,
            else => abi.gfx_mode_phase_reverting,
        };
        self.offered = false;
        self.taken = false;
        self.reply = null;
        self.expired = false;
    }
    pub fn needsStart(self: *const State) bool {
        return !self.offered and self.reply == null and (self.status.phase == abi.gfx_mode_phase_queued or
            self.status.phase == abi.gfx_mode_phase_confirming or self.status.phase == abi.gfx_mode_phase_reverting);
    }
    pub fn arm(self: *State) Error!void {
        if (!self.needsStart()) return error.Busy;
        self.offered = true;
    }
    pub fn take(self: *State, driver: Owner, backend: abi.GfxBackendBinding) Error!?abi.GfxDriverModeJob {
        if (!self.driver.eql(driver) or !std.meta.eql(self.job.backend, backend)) return error.Stale;
        if (!self.offered or self.taken or self.reply != null) return null;
        self.taken = true;
        if (self.job.operation == abi.gfx_mode_operation_apply) self.status.phase = abi.gfx_mode_phase_executing;
        return self.job;
    }
    pub fn complete(self: *State, driver: Owner, value: abi.GfxDriverModeCompletion) Error!void {
        if (value.version != 1 or value.size < @sizeOf(abi.GfxDriverModeCompletion) or value.quiesced & ~@as(u32, 3) != 0 or value.error_code > 0) return error.Invalid;
        if (!self.driver.eql(driver) or !self.offered or !self.taken or self.reply != null or
            value.ticket != self.job.ticket or value.sequence != self.job.sequence or value.operation != self.job.operation) return error.Stale;
        switch (value.outcome) {
            abi.gfx_output_outcome_applied => {
                if (value.operation == abi.gfx_mode_operation_rollback or value.quiesced != 1 or value.error_code != 0) return error.Invalid;
            },
            abi.gfx_output_outcome_old_preserved => {
                if (value.operation == abi.gfx_mode_operation_confirm or value.quiesced != 2) return error.Invalid;
            },
            abi.gfx_output_outcome_lost => {},
            else => return error.Invalid,
        }
        self.reply = value;
    }
    pub fn resolve(self: *State, caller: Owner, ticket: u64, action: u32, now: u64) Error!void {
        if (ticket == 0 or ticket != self.status.ticket or !self.caller.eql(caller)) return error.Stale;
        if (action != abi.gfx_mode_resolve_confirm and action != abi.gfx_mode_resolve_rollback) return error.Invalid;
        if (self.available()) return error.Stale;
        if (action == abi.gfx_mode_resolve_confirm) {
            if (self.status.phase != abi.gfx_mode_phase_awaiting_confirmation or self.cancelled or
                now >= self.status.confirmation_deadline_ns) return error.Busy;
            self.schedule(abi.gfx_mode_operation_confirm, now);
        } else {
            if (self.status.phase == abi.gfx_mode_phase_confirming) return error.Busy;
            self.cancelled = true;
            if (self.status.phase == abi.gfx_mode_phase_awaiting_confirmation)
                self.schedule(abi.gfx_mode_operation_rollback, now);
        }
    }
    pub fn deadline(self: *const State) u64 {
        if (self.status.phase == abi.gfx_mode_phase_awaiting_confirmation) return self.status.confirmation_deadline_ns;
        return self.status.operation_deadline_ns;
    }
    pub fn expire(self: *State, now: u64) bool {
        const end = self.deadline();
        if (end == 0 or now < end or self.reply != null) return false;
        self.cancelled = true;
        self.status.error_code = abi.gfx_output_error_timeout;
        if (self.status.phase == abi.gfx_mode_phase_awaiting_confirmation or
            (!self.taken and self.job.operation == abi.gfx_mode_operation_confirm)) {
            self.schedule(abi.gfx_mode_operation_rollback, now);
        } else if (!self.taken and self.job.operation == abi.gfx_mode_operation_apply) {
            self.rejectUntaken(abi.gfx_output_error_timeout);
        } else {
            // An in-flight receipt may arrive later. Preserve its identity
            // and references; do not issue a competing rollback operation.
            self.expired = true;
            self.status.phase = abi.gfx_mode_phase_lost;
            self.status.outcome = abi.gfx_output_outcome_lost;
            self.status.operation_deadline_ns = 0;
        }
        return true;
    }
    pub fn rejectUntaken(self: *State, error_code: i32) void {
        std.debug.assert(!self.taken and self.reply == null);
        self.reply = .{ .ticket = self.job.ticket, .sequence = self.job.sequence, .operation = self.job.operation,
            .outcome = if (self.job.operation == abi.gfx_mode_operation_apply) abi.gfx_output_outcome_old_preserved else abi.gfx_output_outcome_lost,
            .quiesced = if (self.job.operation == abi.gfx_mode_operation_apply) 2 else 0, .error_code = error_code };
    }
    pub fn settled(self: *State, result: abi.GfxAtomicResult, now: u64) void {
        const value = self.reply.?;
        const operation = self.job.operation;
        self.status.topology_revision = result.topology_revision;
        self.status.commit_sequence = result.commit_sequence;
        self.status.outcome = result.outcome;
        if (value.error_code != 0) self.status.error_code = value.error_code;
        self.status.operation_deadline_ns = 0;
        self.offered = false;
        self.reply = null;
        if (result.outcome == abi.gfx_output_outcome_lost) {
            self.status.phase = abi.gfx_mode_phase_lost;
            self.status.retained = 3;
        } else if (result.outcome == abi.gfx_output_outcome_old_preserved) {
            self.status.phase = abi.gfx_mode_phase_reverted;
            self.status.retained = 1;
        } else if (operation == abi.gfx_mode_operation_confirm) {
            self.status.phase = abi.gfx_mode_phase_confirmed;
            self.status.retained = 2;
        } else {
            self.status.retained = 3;
            if (self.cancelled or self.expired) self.schedule(abi.gfx_mode_operation_rollback, now) else {
                self.status.phase = abi.gfx_mode_phase_awaiting_confirmation;
                self.status.confirmation_deadline_ns = now +| self.confirmation_ns;
            }
        }
    }
};

test "mode jobs bind owner generation and operation, preserve late receipts and automatically request rollback" {
    const t = std.testing;
    const caller: Owner = .{ .kind = .program, .id = 7, .generation = 9 };
    const driver: Owner = .{ .kind = .driver, .id = 12, .generation = 3 };
    var state: State = .{};
    const job: abi.GfxDriverModeJob = .{ .ticket = 4, .backend = .{ .adapter_id = 5, .device_generation = 6, .reset_generation = 7 } };
    try state.begin(caller, driver, job, 8, 15000, 100);
    try t.expect((try state.take(driver, job.backend)) == null);
    try state.arm();
    var wrong = driver; wrong.generation += 1;
    try t.expectError(error.Stale, state.take(wrong, job.backend));
    const first = (try state.take(driver, job.backend)).?;
    try t.expect((try state.take(driver, job.backend)) == null);
    var receipt: abi.GfxDriverModeCompletion = .{ .ticket = first.ticket, .sequence = first.sequence, .operation = first.operation,
        .outcome = abi.gfx_output_outcome_applied, .quiesced = 0 };
    try t.expectError(error.Invalid, state.complete(driver, receipt));
    receipt.quiesced = 1;
    try t.expect(state.expire(first.deadline_ns));
    try t.expect(state.status.phase == abi.gfx_mode_phase_lost and state.status.retained == 3 and !state.available());
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
    state.settled(.{ .outcome = 1 }, 500);
    const end = state.status.confirmation_deadline_ns;
    try t.expectError(error.Stale, state.resolve(driver, next.ticket, abi.gfx_mode_resolve_confirm, end - 1));
    try t.expectError(error.Busy, state.resolve(caller, next.ticket, abi.gfx_mode_resolve_confirm, end));
    try t.expect(state.expire(end) and state.status.phase == abi.gfx_mode_phase_reverting);

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
}
