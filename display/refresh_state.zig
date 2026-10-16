//! Fixed private C state; one native receipt/intent owner, no Zig optional layout.
const std = @import("std");
pub const a = @import("r4os_kernel_contract");
pub const Actor = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Exhausted };
pub const lease_ns: u64 = 1_500_000_000;
extern fn r4os_rust_display_refresh_publish(*State, *const a.GfxOutputRefresh, *bool) i32;
extern fn r4os_rust_display_refresh_get(*const State, *const a.GfxOutputTarget, *a.GfxOutputRefresh) i32;
extern fn r4os_rust_display_refresh_read(*State, *const a.GfxOutputTarget, u64, *a.GfxRefreshRequest) i32;
extern fn r4os_rust_display_refresh_request(*State, *const Actor, *const a.GfxRefreshRequest, u64, *a.GfxRefreshRequest) i32;
extern fn r4os_rust_display_refresh_stopped(*State, *const Actor) bool;
extern fn r4os_rust_display_refresh_pause(*State) void;
fn status(code: i32) Error!void { return switch (code) {
    0 => {}, 1 => error.Invalid, 3 => error.Unsupported, 4 => error.Exhausted,
    7 => error.Stale, 8 => error.Busy, else => unreachable,
}; }
pub const State = extern struct {
    has_value: bool = false, value_data: a.GfxOutputRefresh = .{},
    has_actor: bool = false, actor_data: Actor = .{ .kind = .kernel, .id = 0, .generation = 0 },
    intent: a.GfxRefreshRequest = .{}, serial: u64 = 0,
    pub fn currentValue(self: *const State) ?a.GfxOutputRefresh { return if (self.has_value) self.value_data else null; }
    pub fn actorValue(self: *const State) ?Actor { return if (self.has_actor) self.actor_data else null; }
    pub fn publish(self: *State, input: a.GfxOutputRefresh) Error!bool {
        var out: bool = undefined; try status(r4os_rust_display_refresh_publish(self, &input, &out)); return out;
    }
    pub fn get(self: *const State, identity: a.GfxOutputTarget) Error!a.GfxOutputRefresh {
        var out: a.GfxOutputRefresh = undefined; try status(r4os_rust_display_refresh_get(self, &identity, &out)); return out;
    }
    pub fn read(self: *State, identity: a.GfxOutputTarget, now: u64) Error!a.GfxRefreshRequest {
        var out: a.GfxRefreshRequest = undefined; try status(r4os_rust_display_refresh_read(self, &identity, now, &out)); return out;
    }
    pub fn request(self: *State, caller: Actor, input: a.GfxRefreshRequest, now: u64) Error!a.GfxRefreshRequest {
        var out: a.GfxRefreshRequest = undefined; try status(r4os_rust_display_refresh_request(self, &caller, &input, now, &out)); return out;
    }
    pub fn stopped(self: *State, actor: Actor) bool { return r4os_rust_display_refresh_stopped(self, &actor); }
    pub fn pause(self: *State) void { r4os_rust_display_refresh_pause(self); }
};
comptime {
    if (@sizeOf(State) != 408 or @alignOf(State) != 8 or @offsetOf(State, "value_data") != 8 or @offsetOf(State, "intent") != 312 or @offsetOf(State, "serial") != 400) @compileError("Native refresh intent C layout");
}
