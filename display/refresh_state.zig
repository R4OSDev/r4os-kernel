//! Copied output metadata and one finite presenter lease. No policy engine,
//! hardware access, allocation, callbacks or waiting under the display owner.
const std = @import("std");
pub const a = @import("r4os_kernel_contract");
const target = @import("output_target.zig");
pub const Actor = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Exhausted };
pub const lease_ns = 1_500_000_000;
pub const State = struct {
    value: ?a.GfxOutputRefresh = null,
    actor: ?Actor = null,
    intent: a.GfxRefreshRequest = .{},
    serial: u64 = 0,

    pub fn publish(self: *State, input: a.GfxOutputRefresh) Error!bool {
        if (!valid(input)) return error.Invalid;
        if (input.status.request_sequence > self.serial) return error.Stale;
        if (self.value) |old| {
            if (!std.meta.eql(old.target, input.target)) {
                // A fresh confirmed mode discards old presenter intent.
                self.off();
            } else {
                if (input.status.sequence < old.status.sequence or
                    (input.status.sequence == old.status.sequence and !std.meta.eql(input.status, old.status)) or
                    input.measured.sequence < old.measured.sequence) return error.Stale;
                if (input.measured.sequence == old.measured.sequence and !std.meta.eql(input.measured, old.measured)) return error.Stale;
            }
        }
        var value = input;
        value.size = @sizeOf(a.GfxOutputRefresh);
        value.capabilities.size = @sizeOf(a.GfxRefreshCapabilities);
        value.status.size = @sizeOf(a.GfxRefreshStatus);
        value.measured.size = @sizeOf(a.GfxRefreshMeasure);
        const changed = if (self.value) |old| !std.meta.eql(old.target, value.target) or
            !std.meta.eql(old.capabilities, value.capabilities) or old.status.phase != value.status.phase or
            old.status.reason != value.status.reason or old.status.policy != value.status.policy or
            old.status.scene != value.status.scene or old.status.request_sequence != value.status.request_sequence else true;
        self.value = value;
        return changed;
    }
    pub fn get(self: *const State, output: a.GfxOutputTarget) Error!a.GfxOutputRefresh {
        const value = self.value orelse return error.Unsupported;
        if (!std.meta.eql(value.target, output)) return error.Stale;
        return value;
    }
    fn off(self: *State) void {
        if (self.actor != null or self.intent.policy != 0 or self.intent.scene != 0 or self.intent.operation != 0) {
            self.serial +|= 1;
            self.intent = .{ .target = self.intent.target, .sequence = self.serial };
        }
        self.actor = null;
    }
    pub fn stopped(self: *State, actor: Actor) bool {
        if (self.actor == null or !self.actor.?.eql(actor)) return false;
        self.off();
        return true;
    }
    pub fn pause(self: *State) void { self.off(); }
    pub fn read(self: *State, output: a.GfxOutputTarget, now: u64) Error!a.GfxRefreshRequest {
        _ = try self.get(output);
        if (now == 0 or now == std.math.maxInt(u64)) return error.Invalid;
        if (self.actor != null and now >= self.intent.deadline_ns) self.off();
        var value = self.intent;
        value.target = output;
        return value;
    }
    pub fn request(self: *State, caller: Actor, input: a.GfxRefreshRequest, now: u64) Error!a.GfxRefreshRequest {
        if (!header(input) or !caller.valid() or caller.kind != .program or !target.valid(input.target) or
            input.operation > a.gfx_refresh_operation_clear_fault or input.policy > a.gfx_refresh_policy_windows or
            input.scene & ~@as(u32, 7) != 0 or input.reserved0 != 0 or input.sequence != 0 or input.deadline_ns != 0 or
            now == 0 or now >= std.math.maxInt(u64) - lease_ns) return error.Invalid;
        const current = try self.get(input.target);
        if (input.operation != a.gfx_refresh_operation_configure and (input.policy != 0 or input.scene != 0)) return error.Invalid;
        if (input.policy != 0 and current.capabilities.flags & a.gfx_refresh_cap_capable == 0) return error.Unsupported;
        if (self.actor != null and now < self.intent.deadline_ns and !self.actor.?.eql(caller)) return error.Busy;
        const renewed = self.actor != null and self.actor.?.eql(caller) and now < self.intent.deadline_ns and
            input.operation == a.gfx_refresh_operation_configure and self.intent.operation == input.operation and
            self.intent.policy == input.policy and self.intent.scene == input.scene and std.meta.eql(self.intent.target, input.target);
        if (!renewed and self.serial == std.math.maxInt(u64)) return error.Exhausted;
        if (!renewed) self.serial += 1;
        self.intent = input;
        self.intent.size = @sizeOf(a.GfxRefreshRequest);
        self.intent.sequence = self.serial;
        self.intent.deadline_ns = now + lease_ns;
        self.actor = caller;
        if (input.operation == a.gfx_refresh_operation_release) {
            self.actor = null;
            self.intent.operation = 0;
            self.intent.deadline_ns = 0;
        }
        return self.intent;
    }
};
fn header(value: anytype) bool { return value.version == 1 and value.size >= @sizeOf(@TypeOf(value)); }
fn valid(value: a.GfxOutputRefresh) bool {
    const cap = value.capabilities;
    const status = value.status;
    const measured = value.measured;
    if (!header(value) or !header(cap) or !header(status) or !header(measured) or !target.valid(value.target) or
        value.target.adapter_id == 0 or cap.flags & ~@as(u32, 31) != 0 or cap.flags & a.gfx_refresh_cap_known == 0 or
        cap.origin > 4 or cap.reserved0 != 0 or cap.reserved1 != 0 or status.sequence == 0 or status.phase > 5 or
        status.reason > a.gfx_refresh_reason_stale_clock or status.policy > 2 or status.scene & ~@as(u32, 7) != 0 or
        measured.reserved0 != 0 or measured.reserved1 != 0 or measured.samples > 32) return false;
    if (cap.flags & a.gfx_refresh_cap_capable != 0) {
        if (cap.origin == 0 or cap.min_millihz == 0 or cap.max_millihz < cap.min_millihz or
            cap.nominal_millihz < cap.max_millihz or cap.min_period_ns == 0 or cap.max_period_ns <= cap.min_period_ns or
            cap.max_vtotal == 0 or cap.max_vtotal > 131072) return false;
    } else if (cap.origin != 0 or cap.min_millihz != 0 or cap.max_millihz != 0 or cap.min_period_ns != 0 or
        cap.max_period_ns != 0 or cap.max_increase_ns != 0 or cap.max_decrease_ns != 0 or cap.max_vtotal != 0 or
        cap.flags & a.gfx_refresh_cap_lfc != 0 or status.phase == a.gfx_refresh_phase_active) return false;
    if (status.phase == a.gfx_refresh_phase_active and (status.core_point == 0 or status.receipt == 0 or status.since_ns == 0)) return false;
    if (measured.samples == 0) return measured.last_period_ns == 0 and measured.min_period_ns == 0 and measured.max_period_ns == 0 and
        measured.mean_period_ns == 0 and measured.millihz == 0;
    return measured.sequence != 0 and measured.observed_ns != 0 and measured.min_period_ns != 0 and
        measured.last_period_ns >= measured.min_period_ns and measured.last_period_ns <= measured.max_period_ns and
        measured.mean_period_ns >= measured.min_period_ns and measured.mean_period_ns <= measured.max_period_ns and
        1_000_000_000_000 / measured.mean_period_ns == measured.millihz;
}
