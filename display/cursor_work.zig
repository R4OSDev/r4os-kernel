//! Projection of the sole native cursor transport and image owner.
const model = @import("cursor_state.zig");
pub const a = model.a;
const buffers = @import("../memory/gfx_buffers.zig");
const native = @import("native_driver.zig");
const queue = @import("queue.zig");
pub const Error = native.Error;
extern fn r4os_rust_display_cursor_work(u32, ?*const buffers.Owner, ?*const anyopaque, ?*anyopaque) i32;
pub fn code(err: anyerror) i32 {
    return switch (err) {
        error.Busy => a.gfx_output_error_busy,
        error.Stale, error.WrongOwner, error.Closed => a.gfx_output_error_stale,
        error.Unsupported, error.Disabled => a.gfx_output_error_unsupported,
        error.OutOfMemory, error.Capacity, error.Budget, error.Exhausted, error.Overflow => a.gfx_output_error_capacity,
        error.Unavailable => a.gfx_output_error_unavailable,
        else => a.gfx_output_error_invalid,
    };
}
fn checked(value: i32) Error!void {
    return switch (value) {
        0 => {},
        1 => error.Invalid,
        2 => error.Overflow,
        3 => error.Unsupported,
        4 => error.Exhausted,
        5 => error.Budget,
        6 => error.Capacity,
        7 => error.Stale,
        8 => error.Busy,
        9 => error.Closed,
        10 => error.WrongOwner,
        11 => error.OutOfMemory,
        12 => error.Unavailable,
        13 => error.WaitTimeout,
        14 => error.WaitCancelled,
        15 => error.Disabled,
        16 => error.AlreadyCompleted,
        17 => error.Bandwidth,
        18 => error.Routing,
        19 => error.Dependency,
        20 => error.DeviceLost,
        21 => error.RestoreFailed,
        else => unreachable,
    };
}
pub fn configure(driver: buffers.Owner, input: *const a.DisplayCursorInfo) i32 {
    return r4os_rust_display_cursor_work(0, &driver, input, null);
}
pub fn info(output: *a.DisplayCursorInfo) callconv(.c) i32 {
    return r4os_rust_display_cursor_work(1, null, null, output);
}
pub fn status(output: *a.DisplayCursorStatus) callconv(.c) i32 {
    return r4os_rust_display_cursor_work(2, null, null, output);
}
pub fn submit(caller: buffers.Owner, input: *const a.DisplayCursorRequest, output: *a.DisplayCursorStatus) i32 {
    return r4os_rust_display_cursor_work(3, &caller, input, output);
}
pub fn take(driver: buffers.Owner, backend: *const a.GfxBackendBinding, output: *a.GfxDriverCursorJob) i32 {
    return r4os_rust_display_cursor_work(4, &driver, backend, output);
}
pub fn complete(driver: buffers.Owner, input: *const a.GfxDriverCursorCompletion) i32 {
    return r4os_rust_display_cursor_work(5, &driver, input, null);
}
pub fn stopped(caller: buffers.Owner) void {
    _ = r4os_rust_display_cursor_work(6, &caller, null, null);
}
pub fn stoppedDriver(id: u32) void {
    const driver = buffers.Owner{ .kind = .driver, .id = id, .generation = 0 };
    _ = r4os_rust_display_cursor_work(7, &driver, null, null);
}
pub fn retireAfterReset(driver: buffers.Owner, backend: a.GfxBackendBinding) Error!void {
    try checked(r4os_rust_display_cursor_work(8, &driver, &backend, null));
}
pub fn beforeMode(driver: buffers.Owner) Error!void {
    try checked(r4os_rust_display_cursor_work(9, &driver, null, null));
}
pub fn resumeModes() void {
    _ = r4os_rust_display_cursor_work(10, null, null, null);
}
const CursorBinding = extern struct { driver: buffers.Owner, backend: a.GfxBackendBinding, generation: u64, head: u32, timeline: u64, point: u64 };

comptime {
    if (@sizeOf(CursorBinding) != 88) @compileError("Native cursor runtime binding layout");
}
