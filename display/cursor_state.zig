//! Fixed private C metadata. Explicit presence never lends a Zig optional layout.
pub const a = @import("r4os_kernel_contract");
pub const Identity = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Exhausted, Unavailable };
pub const operation_ns: u64 = 3 * @import("std").time.ns_per_s;
pub const State = extern struct {
    info: a.DisplayCursorInfo = .{}, driver: Identity = .{ .kind = .driver, .id = 0, .generation = 0 },
    has_actor: bool = false, actor_data: Identity = .{ .kind = .kernel, .id = 0, .generation = 0 },
    status: a.DisplayCursorStatus = .{}, has_job: bool = false, job_data: a.GfxDriverCursorJob = .{},
    has_reply: bool = false, reply_data: a.GfxDriverCursorCompletion = .{}, taken: bool = false, closing: bool = false, suspended: bool = false,
    pub fn actorValue(self: *const State) ?Identity { return if (self.has_actor) self.actor_data else null; }
    pub fn jobValue(self: *const State) ?a.GfxDriverCursorJob { return if (self.has_job) self.job_data else null; }
    pub fn replyValue(self: *const State) ?a.GfxDriverCursorCompletion { return if (self.has_reply) self.reply_data else null; }
    pub fn configure(self: *State, driver: Identity, input: a.DisplayCursorInfo) Error!void { try status(r4os_rust_display_cursor_configure(self, &driver, &input)); }
    pub fn visible(self: *const State) bool { return r4os_rust_display_cursor_visible(self); }
    pub fn lost(self: *const State) bool { return r4os_rust_display_cursor_lost(self); }
    pub fn available(self: *const State) bool { return r4os_rust_display_cursor_available(self); }
    pub fn validate(self: *const State, caller: Identity, request: a.DisplayCursorRequest) Error!void { try status(r4os_rust_display_cursor_validate(self, &caller, &request)); }
    pub fn begin(self: *State, caller: Identity, request: a.DisplayCursorRequest, reference: a.GfxBufferHandle, timeline: u64, point: u64, now: u64) Error!void {
        try status(r4os_rust_display_cursor_begin(self, &caller, &request, &reference, timeline, point, now));
    }
    pub fn take(self: *State, driver: Identity, backend: a.GfxBackendBinding, now: u64) Error!?a.GfxDriverCursorJob {
        var out: a.GfxDriverCursorJob = undefined; var has: bool = undefined;
        try status(r4os_rust_display_cursor_take(self, &driver, &backend, now, &out, &has)); return if (has) out else null;
    }
    pub fn validateReply(self: *const State, driver: Identity, value: a.GfxDriverCursorCompletion) Error!bool {
        var out: bool = undefined; try status(r4os_rust_display_cursor_validate_reply(self, &driver, &value, &out)); return out;
    }
    pub fn finish(self: *State, value: a.GfxDriverCursorCompletion) void { r4os_rust_display_cursor_finish(self, &value); }
    pub fn rejectUntaken(self: *State, code: i32) void { r4os_rust_display_cursor_reject(self, code); }
    pub fn close(self: *State, caller: Identity) bool { return r4os_rust_display_cursor_close(self, &caller); }
    pub fn markLost(self: *State, code: i32) void { r4os_rust_display_cursor_mark_lost(self, code); }
    pub fn retireAfterReset(self: *State, driver: Identity, backend: a.GfxBackendBinding) Error!void { try status(r4os_rust_display_cursor_retire(self, &driver, &backend)); }
};
fn status(code: i32) Error!void { return switch (code) { 0 => {}, 1 => error.Invalid, 3 => error.Unsupported, 4 => error.Exhausted, 7 => error.Stale, 8 => error.Busy, 12 => error.Unavailable, else => unreachable }; }
extern fn r4os_rust_display_cursor_configure(*State, *const Identity, *const a.DisplayCursorInfo) i32;
extern fn r4os_rust_display_cursor_visible(*const State) bool;
extern fn r4os_rust_display_cursor_lost(*const State) bool;
extern fn r4os_rust_display_cursor_available(*const State) bool;
extern fn r4os_rust_display_cursor_validate(*const State, *const Identity, *const a.DisplayCursorRequest) i32;
extern fn r4os_rust_display_cursor_begin(*State, *const Identity, *const a.DisplayCursorRequest, *const a.GfxBufferHandle, u64, u64, u64) i32;
extern fn r4os_rust_display_cursor_take(*State, *const Identity, *const a.GfxBackendBinding, u64, *a.GfxDriverCursorJob, *bool) i32;
extern fn r4os_rust_display_cursor_validate_reply(*const State, *const Identity, *const a.GfxDriverCursorCompletion, *bool) i32;
extern fn r4os_rust_display_cursor_finish(*State, *const a.GfxDriverCursorCompletion) void;
extern fn r4os_rust_display_cursor_reject(*State, i32) void;
extern fn r4os_rust_display_cursor_close(*State, *const Identity) bool;
extern fn r4os_rust_display_cursor_mark_lost(*State, i32) void;
extern fn r4os_rust_display_cursor_retire(*State, *const Identity, *const a.GfxBackendBinding) i32;
comptime {
    if (@sizeOf(State) != 432 or @alignOf(State) != 8 or @offsetOf(State, "actor_data") != 112 or @offsetOf(State, "status") != 136 or @offsetOf(State, "job_data") != 216 or @offsetOf(State, "reply_data") != 384 or @offsetOf(State, "taken") != 424) @compileError("Native cursor state C layout");
}
