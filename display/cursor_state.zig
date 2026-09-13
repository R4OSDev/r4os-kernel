//! One primary cursor, one finite job. Metadata only: no buffer calls,
//! callbacks, allocation or waiting while the display state owner is held.
const std = @import("std");
pub const a = @import("r4os_kernel_contract");
pub const Identity = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Exhausted, Unavailable };
pub const operation_ns = 3 * std.time.ns_per_s;
pub const State = struct {
    info: a.DisplayCursorInfo = .{},
    driver: Identity = .{ .kind = .driver, .id = 0, .generation = 0 },
    actor: ?Identity = null,
    status: a.DisplayCursorStatus = .{},
    job: ?a.GfxDriverCursorJob = null,
    reply: ?a.GfxDriverCursorCompletion = null,
    taken: bool = false,
    closing: bool = false,
    suspended: bool = false,

    pub fn configure(self: *State, driver: Identity, input: a.DisplayCursorInfo) Error!void {
        if (input.version != 1 or input.size < @sizeOf(a.DisplayCursorInfo) or input.display_generation == 0 or
            driver.kind != .driver or !driver.valid() or input.backend.version != 1 or input.backend.size < @sizeOf(a.GfxBackendBinding) or
            input.backend.adapter_id == 0 or input.backend.device_generation == 0 or input.backend.reset_generation == 0 or
            input.backend.milestone != a.gfx_queue_milestone_device_execution or input.head_id >= 8) return error.Invalid;
        if (self.info.display_generation != 0 and (!self.driver.eql(driver) or self.info.display_generation != input.display_generation or
            !std.meta.eql(self.info.backend, input.backend) or self.info.head_id != input.head_id)) return error.Stale;
        if (input.flags != 0 and (input.flags != 15 or input.max_width == 0 or input.max_width > 256 or
            input.max_height == 0 or input.max_height > 256 or input.min_x > 0 or input.min_y > 0 or input.max_x <= 0 or input.max_y <= 0)) return error.Invalid;
        if (input.flags == 0 and (self.job != null or self.actor != null or self.visible() or self.lost())) return error.Busy;
        if (self.info.flags != 0 and input.flags != 0 and !std.meta.eql(self.info, input)) return error.Stale;
        self.info = input; self.info.size = @sizeOf(a.DisplayCursorInfo); self.driver = driver;
        self.status.display_generation = input.display_generation; self.status.head_id = input.head_id;
    }
    pub fn visible(self: *const State) bool { return self.status.flags & a.display_cursor_state_visible != 0; }
    pub fn lost(self: *const State) bool { return self.status.phase == a.display_cursor_phase_lost; }
    pub fn available(self: *const State) bool { return self.info.flags != 0 and !self.lost(); }
    pub fn validate(self: *const State, caller: Identity, request: a.DisplayCursorRequest) Error!void {
        if (!self.available()) return error.Unsupported;
        if (!caller.valid() or caller.kind != .program or request.version != 1 or request.size < @sizeOf(a.DisplayCursorRequest) or
            request.operation > a.display_cursor_operation_release) return error.Invalid;
        if (request.display_generation != self.info.display_generation or request.head_id != self.info.head_id) return error.Stale;
        if (self.job != null or self.closing or self.suspended) return error.Busy;
        if (self.actor) |actor| { if (!actor.eql(caller)) return error.Busy; }
        if (self.status.sequence == std.math.maxInt(u64)) return error.Exhausted;
        if (request.operation == a.display_cursor_operation_prepare) {
            if (self.visible()) return error.Busy;
            if (request.reference.id == 0 or request.reference.generation == 0 or request.reference.reserved0 != 0 or
                request.image_sequence != 0 or request.x != 0 or request.y != 0 or request.width == 0 or request.height == 0 or
                request.width > self.info.max_width or request.height > self.info.max_height or
                request.hotspot_x >= request.width or request.hotspot_y >= request.height or
                request.pitch & 3 != 0 or request.pitch < @as(u64, request.width) * 4 or
                request.pitch > std.math.maxInt(u64) / @as(u64, request.height) or request.pitch * request.height != request.byte_length) return error.Invalid;
        } else {
            if (!std.meta.eql(request.reference, a.GfxBufferHandle{}) or request.width != 0 or request.height != 0 or
                request.pitch != 0 or request.byte_length != 0 or request.hotspot_x != 0 or request.hotspot_y != 0) return error.Invalid;
            if (self.actor == null) return error.Stale;
            if (request.operation == a.display_cursor_operation_show or request.operation == a.display_cursor_operation_move) {
                if (request.image_sequence == 0 or request.image_sequence != self.status.image_sequence) return error.Stale;
                if (request.operation == a.display_cursor_operation_move and !self.visible()) return error.Invalid;
                if (request.x < self.info.min_x or request.y < self.info.min_y or request.x > self.info.max_x or request.y > self.info.max_y) return error.Invalid;
            } else if (request.image_sequence != 0 or request.x != 0 or request.y != 0) return error.Invalid;
        }
    }
    pub fn begin(self: *State, caller: Identity, request: a.DisplayCursorRequest, reference: a.GfxBufferHandle,
        timeline: u64, point: u64, now: u64) Error!void
    {
        try self.validate(caller, request);
        if (now == 0 or now >= std.math.maxInt(u64) - operation_ns or (timeline == 0) != (point == 0)) return error.Invalid;
        if (request.operation == a.display_cursor_operation_prepare and (reference.id == 0 or reference.generation == 0 or
            reference.reserved0 != 0 or std.meta.eql(reference, request.reference))) return error.Invalid;
        self.actor = caller;
        self.arm(request, now);
        if (request.operation == a.display_cursor_operation_prepare) self.job.?.request.reference = reference;
        if (request.operation == a.display_cursor_operation_show) {
            self.job.?.barrier_timeline = timeline; self.job.?.barrier_point = point;
        }
    }
    fn arm(self: *State, request: a.DisplayCursorRequest, now: u64) void {
        self.status.sequence += 1; self.status.phase = a.display_cursor_phase_queued;
        self.status.deadline_ns = now + operation_ns; self.status.error_code = 0;
        self.status.flags |= a.display_cursor_state_claimed;
        self.job = .{ .backend = self.info.backend, .sequence = self.status.sequence,
            .deadline_ns = self.status.deadline_ns, .request = request };
        self.job.?.request.size = @sizeOf(a.DisplayCursorRequest);
        self.taken = false; self.reply = null;
    }
    pub fn take(self: *State, driver: Identity, backend: a.GfxBackendBinding, now: u64) Error!?a.GfxDriverCursorJob {
        if (!self.driver.eql(driver) or !std.meta.eql(self.info.backend, backend)) return error.Stale;
        if (!self.available()) return null;
        if (self.job == null and (self.closing or (self.suspended and self.visible()))) {
            if (self.status.sequence == std.math.maxInt(u64) or now == 0 or now >= std.math.maxInt(u64) - operation_ns) return error.Exhausted;
            self.arm(.{ .display_generation = self.info.display_generation, .head_id = self.info.head_id,
                .operation = if (self.closing) a.display_cursor_operation_release else a.display_cursor_operation_hide }, now);
        }
        const job = self.job orelse return null;
        if (self.taken) return null;
        if (now >= job.deadline_ns) return error.Busy; // Wrapper settles untouched resources first.
        self.taken = true; self.status.phase = a.display_cursor_phase_active;
        return job;
    }
    pub fn validateReply(self: *const State, driver: Identity, value: a.GfxDriverCursorCompletion) Error!bool {
        if (!self.driver.eql(driver) or value.display_generation != self.info.display_generation) return error.Stale;
        if (value.version != 1 or value.size < @sizeOf(a.GfxDriverCursorCompletion) or value.reserved0 != 0 or
            value.sequence == 0 or value.visibility > a.display_cursor_visibility_unknown) return error.Invalid;
        if (self.reply) |previous| {
            var normalized = value; normalized.size = @sizeOf(a.GfxDriverCursorCompletion);
            if (std.meta.eql(previous, normalized)) return false;
        }
        const job = self.job orelse return error.Stale;
        if (!self.taken or value.sequence != job.sequence) return error.Stale;
        if (value.outcome == a.gfx_output_outcome_lost) {
            if (value.error_code >= 0 or value.visibility != a.display_cursor_visibility_unknown) return error.Invalid;
            return true;
        }
        if (value.outcome == a.gfx_output_outcome_old_preserved) {
            if (value.error_code >= 0 or value.visibility != @as(u32, @intFromBool(self.visible()))) return error.Invalid;
        } else if (value.outcome == a.gfx_output_outcome_applied) {
            const shown = job.request.operation == a.display_cursor_operation_show or job.request.operation == a.display_cursor_operation_move;
            if (value.error_code != 0 or value.visibility != @as(u32, @intFromBool(shown))) return error.Invalid;
        } else return error.Invalid;
        return true;
    }
    pub fn finish(self: *State, value: a.GfxDriverCursorCompletion) void {
        self.reply = value; self.reply.?.size = @sizeOf(a.GfxDriverCursorCompletion);
        if (value.outcome == a.gfx_output_outcome_lost) { self.markLost(value.error_code); return; }
        const request = self.job.?.request;
        const applied = value.outcome == a.gfx_output_outcome_applied;
        self.status.flags &= ~a.display_cursor_state_unknown;
        if (applied) switch (request.operation) {
            a.display_cursor_operation_prepare => {
                self.status.image_sequence = self.job.?.sequence;
                self.status.flags |= a.display_cursor_state_image_ready;
            },
            a.display_cursor_operation_show, a.display_cursor_operation_move => {
                self.status.x = request.x; self.status.y = request.y;
                self.status.flags |= a.display_cursor_state_visible;
            },
            a.display_cursor_operation_hide, a.display_cursor_operation_release => {
                self.status.flags &= ~a.display_cursor_state_visible;
                if (request.operation == a.display_cursor_operation_release) {
                    self.actor = null; self.status.image_sequence = 0;
                    self.status.flags = if (self.suspended) a.display_cursor_state_suspended else 0;
                    self.closing = false;
                }
            },
            else => unreachable,
        };
        self.status.completed = value.sequence; self.status.error_code = value.error_code;
        self.status.phase = if (applied) a.display_cursor_phase_complete else a.display_cursor_phase_failed;
        self.status.deadline_ns = 0; self.job = null; self.taken = false;
    }
    pub fn rejectUntaken(self: *State, code: i32) void {
        std.debug.assert(self.job != null and !self.taken);
        self.status.completed = self.job.?.sequence; self.status.phase = a.display_cursor_phase_failed;
        self.status.error_code = code; self.status.deadline_ns = 0; self.job = null;
    }
    pub fn close(self: *State, caller: Identity) bool {
        if (self.actor == null or !self.actor.?.eql(caller)) return false;
        self.closing = true; return true;
    }
    pub fn markLost(self: *State, code: i32) void {
        self.status.phase = a.display_cursor_phase_lost; self.status.error_code = code;
        self.status.flags |= a.display_cursor_state_unknown; self.status.deadline_ns = 0;
    }
};
