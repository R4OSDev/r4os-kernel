//! Fixed projection of the sole native asynchronous mode owner.
const model = @import("mode_state.zig");
pub const abi = model.abi;
const buffers = @import("../memory/gfx_buffers.zig");
const native = @import("native_driver.zig");
const queue = @import("queue.zig");
const display = @import("display.zig");
pub const Error = native.Error;
pub const code = native.code;
const Args = extern struct { owner: buffers.Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, backend: abi.GfxBackendBinding = .{}, ticket: u64 = 0, value: u32 = 0, sequence: u64 = 0, input: ?*const anyopaque = null };
extern fn r4os_rust_display_mode_work(u32, *const Args, ?*anyopaque) i32;
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

fn call(op: u32, args: Args) Error!void {
    try checked(r4os_rust_display_mode_work(op, &args, null));
}
fn result(comptime T: type, op: u32, args: Args) Error!T {
    var out: T = undefined;
    try checked(r4os_rust_display_mode_work(op, &args, &out));
    return out;
}
pub fn enable(driver: buffers.Owner, backend: abi.GfxBackendBinding) Error!void {
    try call(0, .{ .owner = driver, .backend = backend });
}
pub fn status(ticket: u64) Error!abi.GfxModeStatus {
    return result(abi.GfxModeStatus, 1, .{ .ticket = ticket });
}
pub fn presentationReady(driver: buffers.Owner, backend: abi.GfxBackendBinding) bool {
    return result(bool, 2, .{ .owner = driver, .backend = backend }) catch unreachable;
}
pub fn submit(caller: buffers.Owner, input: *const abi.GfxAtomicState, confirmation_ms: u32) Error!abi.GfxModeStatus {
    return result(abi.GfxModeStatus, 3, .{ .owner = caller, .input = input, .value = confirmation_ms });
}
pub fn submitColor(caller: buffers.Owner, input: *const abi.GfxModeColorRequest, confirmation_ms: u32) Error!abi.GfxModeStatus {
    return result(abi.GfxModeStatus, 4, .{ .owner = caller, .input = input, .value = confirmation_ms });
}
pub fn restore(driver: buffers.Owner, input: *const abi.GfxAtomicState) Error!abi.GfxModeStatus {
    return result(abi.GfxModeStatus, 5, .{ .owner = driver, .input = input });
}
pub fn driverStatus(driver: buffers.Owner, ticket: u64) Error!abi.GfxModeStatus {
    return result(abi.GfxModeStatus, 6, .{ .owner = driver, .ticket = ticket });
}
pub fn resolve(caller: buffers.Owner, ticket: u64, action: u32) Error!abi.GfxModeStatus {
    return result(abi.GfxModeStatus, 7, .{ .owner = caller, .ticket = ticket, .value = action });
}
const OptionalJob = extern struct { has: bool, value: abi.GfxDriverModeJob };
const OptionalColor = extern struct { has: bool, value: abi.GfxDriverModeColor };
pub fn take(driver: buffers.Owner, backend: abi.GfxBackendBinding) Error!?abi.GfxDriverModeJob {
    const out = try result(OptionalJob, 8, .{ .owner = driver, .backend = backend });
    return if (out.has) out.value else null;
}
pub fn complete(driver: buffers.Owner, value: abi.GfxDriverModeCompletion) Error!void {
    try call(9, .{ .owner = driver, .input = &value });
}
pub fn readColor(driver: buffers.Owner, ticket: u64, sequence: u64) Error!?abi.GfxDriverModeColor {
    const out = try result(OptionalColor, 10, .{ .owner = driver, .ticket = ticket, .sequence = sequence });
    return if (out.has) out.value else null;
}
pub fn retireAfterReset(driver: buffers.Owner, backend: abi.GfxBackendBinding, reset_generation: u64) Error!void {
    try call(11, .{ .owner = driver, .backend = backend, .sequence = reset_generation });
}
pub fn stoppedDriver(id: u32) void {
    call(12, .{ .owner = .{ .kind = .driver, .id = id, .generation = 0 } }) catch unreachable;
}
pub fn withdrawingOutput(driver_id: u32, output: abi.GfxOutputId) bool {
    return result(bool, 13, .{ .owner = .{ .kind = .driver, .id = driver_id, .generation = 0 }, .input = &output }) catch unreachable;
}

const AdditionalBinding = extern struct {
    driver: buffers.Owner = .{ .kind = .kernel, .id = 0, .generation = 0 },
    backend: abi.GfxBackendBinding = .{},
    target: abi.GfxOutputTarget = .{},
    width: u32 = 0,
    height: u32 = 0,
    active: bool = false,
};
const ModeBinding = extern struct { driver: buffers.Owner, backend: abi.GfxBackendBinding, generation: u64, has_additional: bool, additional: AdditionalBinding };
fn wire(value: native.ModeBinding) ModeBinding {
    return .{ .driver = value.driver, .backend = value.backend, .generation = value.generation, .has_additional = value.additional != null, .additional = if (value.additional) |v| .{ .driver = v.driver, .backend = v.backend, .target = v.target, .width = v.width, .height = v.height, .active = v.active } else .{} };
}
fn binding(value: ModeBinding) native.ModeBinding {
    return .{ .driver = value.driver, .backend = value.backend, .generation = value.generation, .additional = if (value.has_additional) .{ .driver = value.additional.driver, .backend = value.additional.backend, .target = value.additional.target, .width = value.additional.width, .height = value.additional.height, .active = value.additional.active } else null };
}
const Prepare = extern struct { assignment: abi.GfxScanoutState, mode: abi.GfxOutputMode, has_format: bool, format: u32 };
