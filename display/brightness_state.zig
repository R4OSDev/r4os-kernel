//! Fixed private C state; one native receipt/intent owner, no Zig optional layout.
const std = @import("std");
pub const a = @import("r4os_kernel_contract");
pub const Actor = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Exhausted };
extern fn r4os_rust_display_brightness_publish(*State, *const a.GfxOutputBrightness, *bool) i32;
extern fn r4os_rust_display_brightness_get(*const State, *const a.GfxOutputId, *a.GfxOutputBrightness) i32;
extern fn r4os_rust_display_brightness_read(*const State, *const a.GfxOutputId, *a.GfxBrightnessRequest) i32;
extern fn r4os_rust_display_brightness_request(*State, *const Actor, *const a.GfxBrightnessRequest, *a.GfxBrightnessRequest) i32;
fn status(code: i32) Error!void { return switch (code) {
    0 => {}, 1 => error.Invalid, 3 => error.Unsupported, 4 => error.Exhausted,
    7 => error.Stale, 8 => error.Busy, else => unreachable,
}; }
pub const State = extern struct {
    has_value: bool = false, value_data: a.GfxOutputBrightness = .{},
    intent: a.GfxBrightnessRequest = .{}, serial: u64 = 0,
    pub fn currentValue(self: *const State) ?a.GfxOutputBrightness { return if (self.has_value) self.value_data else null; }
    pub fn publish(self: *State, input: a.GfxOutputBrightness) Error!bool {
        var out: bool = undefined; try status(r4os_rust_display_brightness_publish(self, &input, &out)); return out;
    }
    pub fn get(self: *const State, identity: a.GfxOutputId) Error!a.GfxOutputBrightness {
        var out: a.GfxOutputBrightness = undefined; try status(r4os_rust_display_brightness_get(self, &identity, &out)); return out;
    }
    pub fn read(self: *const State, identity: a.GfxOutputId) Error!a.GfxBrightnessRequest {
        var out: a.GfxBrightnessRequest = undefined; try status(r4os_rust_display_brightness_read(self, &identity, &out)); return out;
    }
    pub fn request(self: *State, caller: Actor, input: a.GfxBrightnessRequest) Error!a.GfxBrightnessRequest {
        var out: a.GfxBrightnessRequest = undefined; try status(r4os_rust_display_brightness_request(self, &caller, &input, &out)); return out;
    }
};
comptime {
    if (@sizeOf(State) != 152 or @alignOf(State) != 8 or @offsetOf(State, "value_data") != 8 or @offsetOf(State, "intent") != 96 or @offsetOf(State, "serial") != 144) @compileError("Native brightness intent C layout");
}
