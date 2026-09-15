//! Copied screen power receipts and a bounded program intent lease. This
//! owner never touches hardware, suspends a task, calls a driver or frees BOs.
const std = @import("std");
pub const a = @import("r4os_kernel_contract");
pub const Actor = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Exhausted };
pub const lease_ns = 30 * std.time.ns_per_s;
pub const State = struct {
    value: ?a.GfxOutputPower = null,
    actor: ?Actor = null,
    intent: a.GfxPowerRequest = .{},
    serial: u64 = 0,

    pub fn sleeping(self: *const State) bool {
        return if (self.value) |value| sleepingPhase(value.phase) else false;
    }
    pub fn publish(self: *State, input: a.GfxOutputPower) Error!bool {
        if (!header(input) or !validId(input.identity) or input.capabilities & ~@as(u32, 3) != 0 or
            input.phase > a.gfx_power_phase_unavailable or input.sequence == 0 or input.since_ns == 0 or
            input.reason > a.gfx_power_reason_unsupported or input.reserved0 != 0) return error.Invalid;
        if (input.request_sequence > self.serial) return error.Stale;
        if (input.capabilities & a.gfx_power_cap_signal == 0) {
            if (input.capabilities != 0 or input.phase != a.gfx_power_phase_unavailable or
                input.reason != a.gfx_power_reason_unsupported) return error.Invalid;
        }
        if ((input.phase == a.gfx_power_phase_on or input.phase == a.gfx_power_phase_off) and
            (input.core_point == 0 or input.window_point == 0 or input.reason != 0)) return error.Invalid;
        if (input.phase == a.gfx_power_phase_off and input.capabilities & a.gfx_power_cap_sink != 0 and
            input.control_receipt == 0) return error.Invalid;
        if (self.value) |old| {
            if (!std.meta.eql(old.identity, input.identity) or input.sequence < old.sequence or
                input.since_ns < old.since_ns or (input.sequence == old.sequence and !std.meta.eql(old, input))) return error.Stale;
            if (std.meta.eql(old, input)) return false;
        }
        self.value = input;
        return true;
    }
    pub fn get(self: *const State, identity: a.GfxOutputId) Error!a.GfxOutputPower {
        const value = self.value orelse return error.Unsupported;
        if (!std.meta.eql(value.identity, identity)) return error.Stale;
        return value;
    }
    fn wake(self: *State) void {
        if (self.intent.off != 0) {
            self.serial +|= 1;
            self.intent = .{ .identity = self.intent.identity, .sequence = self.serial };
        }
        self.actor = null;
    }
    pub fn stopped(self: *State, actor: Actor) bool {
        if (self.actor == null or !self.actor.?.eql(actor)) return false;
        self.wake();
        return true;
    }
    pub fn read(self: *State, identity: a.GfxOutputId, now: u64) Error!a.GfxPowerRequest {
        _ = try self.get(identity);
        if (now == 0 or now == std.math.maxInt(u64)) return error.Invalid;
        if (self.actor != null and now >= self.intent.deadline_ns) self.wake();
        var value = self.intent;
        value.identity = identity;
        return value;
    }
    pub fn request(self: *State, caller: Actor, input: a.GfxPowerRequest, now: u64) Error!a.GfxPowerRequest {
        if (!header(input) or !caller.valid() or caller.kind != .program or !validId(input.identity) or
            input.off > 1 or input.reserved0 != 0 or input.reserved1 != 0 or input.sequence != 0 or input.deadline_ns != 0 or
            now == 0 or now >= std.math.maxInt(u64) - lease_ns) return error.Invalid;
        const value = try self.get(input.identity);
        if (value.capabilities & a.gfx_power_cap_signal == 0) return error.Unsupported;
        if (input.off != 0 and self.actor != null and now < self.intent.deadline_ns and !self.actor.?.eql(caller)) return error.Busy;
        const renew = input.off != 0 and self.actor != null and self.actor.?.eql(caller) and
            now < self.intent.deadline_ns and self.intent.off == input.off;
        if (!renew and self.serial == std.math.maxInt(u64)) return error.Exhausted;
        if (!renew) self.serial += 1;
        self.intent = input;
        self.intent.size = @sizeOf(a.GfxPowerRequest);
        self.intent.sequence = self.serial;
        self.intent.deadline_ns = if (input.off != 0) now + lease_ns else 0;
        self.actor = if (input.off != 0) caller else null;
        return self.intent;
    }
};
pub fn sleepingPhase(phase: u32) bool { return phase >= a.gfx_power_phase_stopping and phase <= a.gfx_power_phase_waking; }
pub fn validId(identity: a.GfxOutputId) bool {
    return identity.adapter_id != 0 and identity.connector_id != 0 and identity.device_generation != 0 and identity.connection_generation != 0;
}
fn header(value: anytype) bool { return value.version == 1 and value.size == @sizeOf(@TypeOf(value)); }
