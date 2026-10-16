//! Exact native encoded color image owner; no independent shadow or state.
const a = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
pub const Error = @import("outputs.zig").Error;
extern fn r4os_rust_display_mode_color_owner(u32, ?*Owner, ?*const buffers.Owner, ?*const buffers.Owner, ?*const a.GfxModeColorRequest, ?*a.GfxDriverModeColor) i32;
fn status(code: i32) Error!void { return switch (code) {
    0 => {}, 1 => error.Invalid, 2 => error.Overflow, 3 => error.Unsupported, 4 => error.Exhausted, 5 => error.Budget, 6 => error.Capacity,
    7 => error.Stale, 8 => error.Busy, 9 => error.Closed, 10 => error.WrongOwner, 11 => error.OutOfMemory, 12 => error.Unavailable,
    13 => error.WaitTimeout, 14 => error.WaitCancelled, 16 => error.AlreadyCompleted, 17 => error.Bandwidth, 18 => error.Routing, 19 => error.Dependency, 20 => error.DeviceLost, else => unreachable,
}; }
pub fn validate(caller: buffers.Owner, request: *const a.GfxModeColorRequest) Error!void { try status(r4os_rust_display_mode_color_owner(2, null, &caller, null, request, null)); }
pub const Owner = extern struct {
    reference: buffers.Handle = .{}, driver_reference: buffers.Handle = .{}, read: buffers.Handle = .{}, driver: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    pub fn empty(self: *const Owner) bool { return self.reference.id == 0 and self.driver_reference.id == 0 and self.read.id == 0; }
    pub fn prepare(self: *Owner, caller: buffers.Owner, driver: buffers.Owner, request: *const a.GfxModeColorRequest) Error!a.GfxDriverModeColor {
        var out: a.GfxDriverModeColor = undefined; try status(r4os_rust_display_mode_color_owner(0, self, &caller, &driver, request, &out)); return out;
    }
    pub fn release(self: *Owner) Error!void { try status(r4os_rust_display_mode_color_owner(1, self, null, null, null, null)); }
};
comptime { if (@sizeOf(Owner) != 72 or @offsetOf(Owner, "driver") != 48) @compileError("Native mode color image C layout"); }
