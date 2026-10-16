//! Projection of the native exact additional-head BO owner.
const abi = @import("r4os_kernel_contract");
const runtime = @import("output_runtime.zig");
const buffers = @import("../memory/gfx_buffers.zig");
pub const Binding = runtime.ModeBinding;
pub const Error = @import("outputs.zig").Error || @import("output_runtime_state.zig").Error;
const Args = extern struct { owner: buffers.Owner = .{ .kind = .kernel, .id = 0, .generation = 0 },
    expected: Binding = .{ .driver = .{ .kind = .kernel, .id = 0, .generation = 0 }, .backend = .{}, .target = .{}, .width = 0, .height = 0, .active = false },
    target: abi.GfxOutputTarget = .{}, backend: abi.GfxBackendBinding = .{}, assignment: abi.GfxScanoutState = .{}, mode: abi.GfxOutputMode = .{},
    operation: u32 = 0, outcome: u32 = 0 };
extern fn r4os_rust_display_additional_mode(u32, *const Args, ?*anyopaque) i32;
fn checked(value: i32) Error!void { return switch (value) {
    0 => {}, 1 => error.Invalid, 2 => error.Overflow, 3 => error.Unsupported, 4 => error.Exhausted, 5 => error.Budget, 6 => error.Capacity,
    7 => error.Stale, 8 => error.Busy, 9 => error.Closed, 10 => error.WrongOwner, 11 => error.OutOfMemory, 12 => error.Unavailable,
    13 => error.WaitTimeout, 14 => error.WaitCancelled, 16 => error.AlreadyCompleted, 17 => error.Bandwidth,
    18 => error.Routing, 19 => error.Dependency, 20 => error.DeviceLost, else => unreachable,
}; }


fn call(op: u32, args: Args) Error!void { try checked(r4os_rust_display_additional_mode(op, &args, null)); }
pub fn prepare(caller: buffers.Owner, assignment: abi.GfxScanoutState, mode: abi.GfxOutputMode, expected: Binding) Error!abi.GfxBufferReference {
    const args: Args = .{ .owner = caller, .assignment = assignment, .mode = mode, .expected = expected }; var out: abi.GfxBufferReference = undefined;
    try checked(r4os_rust_display_additional_mode(0, &args, &out)); return out;
}
pub fn start(operation: u32, expected: Binding) Error!void { try call(1, .{ .operation = operation, .expected = expected }); }
pub fn settle(operation: u32, outcome: u32) Error!void { try call(2, .{ .operation = operation, .outcome = outcome }); }
pub fn retire(driver: buffers.Owner, target: abi.GfxOutputTarget) Error!void { try call(3, .{ .owner = driver, .target = target }); }
pub fn retireAfterReset(driver: buffers.Owner, backend: abi.GfxBackendBinding) Error!void { try call(4, .{ .owner = driver, .backend = backend }); }
fn peerCode(err: anyerror) i32 { return switch (err) {
    error.Invalid => 1, error.Overflow => 2, error.Unsupported => 3, error.Exhausted => 4, error.Budget => 5, error.Capacity => 6,
    error.Stale => 7, error.Busy => 8, error.Closed => 9, error.WrongOwner => 10, error.OutOfMemory => 11, error.Unavailable => 12,
    error.WaitTimeout => 13, error.WaitCancelled => 14, error.Disabled => 15, error.AlreadyCompleted => 16, error.Bandwidth => 17,
    error.Routing => 18, error.Dependency => 19, error.DeviceLost => 20, error.RestoreFailed => 21, else => unreachable,
}; }
