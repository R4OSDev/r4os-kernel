//! Fixed private C state; one native receipt/intent owner, no Zig optional layout.
const std = @import("std");
pub const a = @import("r4os_kernel_contract");
pub const Actor = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Exhausted };
pub const lease_ns: u64 = 30 * std.time.ns_per_s;
extern fn r4os_rust_display_power_publish(*State, *const a.GfxOutputPower, *bool) i32;
extern fn r4os_rust_display_power_get(*const State, *const a.GfxOutputId, *a.GfxOutputPower) i32;
extern fn r4os_rust_display_power_read(*State, *const a.GfxOutputId, u64, *a.GfxPowerRequest) i32;
extern fn r4os_rust_display_power_request(*State, *const Actor, *const a.GfxPowerRequest, u64, *a.GfxPowerRequest) i32;
extern fn r4os_rust_display_power_stopped(*State, *const Actor) bool;
extern fn r4os_rust_display_power_sleeping(*const State) bool;
extern fn r4os_rust_display_power_phase(u32) bool;
extern fn r4os_rust_display_power_id_valid(*const a.GfxOutputId) bool;
pub fn sleepingPhase(phase: u32) bool { return r4os_rust_display_power_phase(phase); }
pub fn validId(identity: a.GfxOutputId) bool { return r4os_rust_display_power_id_valid(&identity); }
fn status(code: i32) Error!void { return switch (code) {
    0 => {}, 1 => error.Invalid, 3 => error.Unsupported, 4 => error.Exhausted,
    7 => error.Stale, 8 => error.Busy, else => unreachable,
}; }
pub const State = extern struct {
    has_value: bool = false, value_data: a.GfxOutputPower = .{},
    has_actor: bool = false, actor_data: Actor = .{ .kind = .kernel, .id = 0, .generation = 0 },
    intent: a.GfxPowerRequest = .{}, serial: u64 = 0,
    pub fn currentValue(self: *const State) ?a.GfxOutputPower { return if (self.has_value) self.value_data else null; }
    pub fn actorValue(self: *const State) ?Actor { return if (self.has_actor) self.actor_data else null; }
    pub fn publish(self: *State, input: a.GfxOutputPower) Error!bool {
        var out: bool = undefined; try status(r4os_rust_display_power_publish(self, &input, &out)); return out;
    }
    pub fn get(self: *const State, identity: a.GfxOutputId) Error!a.GfxOutputPower {
        var out: a.GfxOutputPower = undefined; try status(r4os_rust_display_power_get(self, &identity, &out)); return out;
    }
    pub fn read(self: *State, identity: a.GfxOutputId, now: u64) Error!a.GfxPowerRequest {
        var out: a.GfxPowerRequest = undefined; try status(r4os_rust_display_power_read(self, &identity, now, &out)); return out;
    }
    pub fn request(self: *State, caller: Actor, input: a.GfxPowerRequest, now: u64) Error!a.GfxPowerRequest {
        var out: a.GfxPowerRequest = undefined; try status(r4os_rust_display_power_request(self, &caller, &input, now, &out)); return out;
    }
    pub fn stopped(self: *State, actor: Actor) bool { return r4os_rust_display_power_stopped(self, &actor); }
    pub fn sleeping(self: *const State) bool { return r4os_rust_display_power_sleeping(self); }
};
comptime {
    if (@sizeOf(State) != 208 or @alignOf(State) != 8 or @offsetOf(State, "value_data") != 8 or @offsetOf(State, "intent") != 136 or @offsetOf(State, "serial") != 200) @compileError("Native power intent C layout");
}
