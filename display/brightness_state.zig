//! Copied intent and confirmed driver receipt. Hardware and preferences belong
//! to the driver and Desktop; this state performs no I/O or callbacks.
const std = @import("std");
pub const a = @import("r4os_kernel_contract");
const Actor = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Unsupported, Exhausted };
pub const State = struct {
    value: ?a.GfxOutputBrightness = null,
    intent: a.GfxBrightnessRequest = .{},
    serial: u64 = 0,

    pub fn publish(self: *State, input: a.GfxOutputBrightness) Error!bool {
        if (!header(input) or !@import("power_state.zig").validId(input.identity) or
            input.path > a.gfx_brightness_path_aux16 or input.phase > a.gfx_brightness_phase_failed or
            input.reason > a.gfx_brightness_reason_inactive or input.flags & ~a.gfx_brightness_flag_current_known != 0 or
            input.reserved0 != 0 or input.sequence == 0 or input.since_ns == 0 or
            input.minimum > input.maximum or input.maximum > 65535 or input.current > 65535) return error.Invalid;
        if (input.request_sequence > self.serial) return error.Stale;
        if (input.phase == a.gfx_brightness_phase_unavailable) {
            if (input.path != 0 or input.reason == 0 or input.flags != 0) return error.Invalid;
        } else {
            if (input.path == 0 or input.minimum == input.maximum) return error.Invalid;
            if (input.phase == a.gfx_brightness_phase_ready and (input.reason != 0 or input.flags == 0)) return error.Invalid;
            if (input.phase == a.gfx_brightness_phase_failed and input.reason == 0) return error.Invalid;
        }
        if (self.value) |old| {
            if (!std.meta.eql(old.identity, input.identity) or input.sequence < old.sequence or
                input.request_sequence < old.request_sequence or input.since_ns < old.since_ns or
                (input.sequence == old.sequence and !std.meta.eql(old, input))) return error.Stale;
            if (std.meta.eql(old, input)) return false;
        }
        self.value = input;
        return true;
    }
    pub fn get(self: *const State, identity: a.GfxOutputId) Error!a.GfxOutputBrightness {
        const value = self.value orelse return error.Unsupported;
        if (!std.meta.eql(value.identity, identity)) return error.Stale;
        return value;
    }
    pub fn read(self: *const State, identity: a.GfxOutputId) Error!a.GfxBrightnessRequest {
        _ = try self.get(identity);
        var value = self.intent;
        value.identity = identity;
        return value;
    }
    pub fn request(self: *State, caller: Actor, input: a.GfxBrightnessRequest) Error!a.GfxBrightnessRequest {
        if (!header(input) or !caller.valid() or caller.kind != .program or input.level > 65535 or
            input.reserved0 != 0 or input.sequence != 0) return error.Invalid;
        const value = try self.get(input.identity);
        if (value.path == 0 or value.phase == a.gfx_brightness_phase_unavailable) return error.Unsupported;
        if (input.level < value.minimum or input.level > value.maximum) return error.Invalid;
        if (self.serial == std.math.maxInt(u64)) return error.Exhausted;
        // Each explicit request has a fresh serial, including retrying a failed
        // level. Desktop deduplicates saved choices instead of silently retrying.
        self.serial += 1;
        self.intent = input;
        self.intent.sequence = self.serial;
        return self.intent;
    }
};
fn header(value: anytype) bool { return value.version == 1 and value.size == @sizeOf(@TypeOf(value)); }
