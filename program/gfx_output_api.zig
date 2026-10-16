//! Temporary fixed projection of the native public output API.
const outputs = @import("../display/outputs.zig");
pub const abi = outputs.abi;
const buffers = @import("../memory/gfx_buffers.zig");
const Args = extern struct { owner: buffers.Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, ticket: u64 = 0, value: u32 = 0, other: u32 = 0, input: ?*const anyopaque = null };
extern fn r4os_rust_gfx_output_api(u32, *const Args, ?*anyopaque) i32;
pub fn code(err: outputs.Error) i32 {
    return r4os_rust_gfx_output_api(17, &Args{ .value = privateCode(err) }, null);
}
pub fn revision(output: *abi.GfxDisplayRevision) callconv(.c) i32 {
    return r4os_rust_gfx_output_api(0, &Args{}, output);
}
pub fn info(index: u32, output: *abi.GfxOutputInfo) callconv(.c) i32 {
    return r4os_rust_gfx_output_api(1, &Args{ .value = index }, output);
}
pub fn mode(input: *const abi.GfxOutputId, index: u32, output: *abi.GfxOutputMode) callconv(.c) i32 {
    return r4os_rust_gfx_output_api(2, &Args{ .input = input, .value = index }, output);
}
pub fn edid(input: *const abi.GfxOutputId, index: u32, output: *abi.GfxEdidBlock) callconv(.c) i32 {
    return r4os_rust_gfx_output_api(3, &Args{ .input = input, .value = index }, output);
}
pub fn color(input: *const abi.GfxOutputId, output: *abi.GfxOutputColorState) callconv(.c) i32 {
    return r4os_rust_gfx_output_api(4, &Args{ .input = input }, output);
}
pub fn refresh(input: *const abi.GfxOutputTarget, output: *abi.GfxOutputRefresh) callconv(.c) i32 {
    return r4os_rust_gfx_output_api(5, &Args{ .input = input }, output);
}
pub fn requestRefresh(owner: buffers.Owner, input: *const abi.GfxRefreshRequest, output: *abi.GfxRefreshRequest) i32 {
    return r4os_rust_gfx_output_api(6, &Args{ .owner = owner, .input = input }, output);
}
pub fn power(input: *const abi.GfxOutputId, output: *abi.GfxOutputPower) callconv(.c) i32 {
    return r4os_rust_gfx_output_api(7, &Args{ .input = input }, output);
}
pub fn requestPower(owner: buffers.Owner, input: *const abi.GfxPowerRequest, output: *abi.GfxPowerRequest) i32 {
    return r4os_rust_gfx_output_api(8, &Args{ .owner = owner, .input = input }, output);
}
pub fn atomic(owner: buffers.Owner, input: *const abi.GfxAtomicState, output: *abi.GfxAtomicResult, commit: bool) i32 {
    return r4os_rust_gfx_output_api(9, &Args{ .owner = owner, .input = input, .other = @intFromBool(commit) }, output);
}
pub fn submit(owner: buffers.Owner, input: *const abi.GfxAtomicState, confirmation_ms: u32, output: *abi.GfxModeStatus) i32 {
    return r4os_rust_gfx_output_api(10, &Args{ .owner = owner, .input = input, .value = confirmation_ms }, output);
}
pub fn testColor(owner: buffers.Owner, input: *const abi.GfxModeColorRequest, output: *abi.GfxAtomicResult) i32 {
    return r4os_rust_gfx_output_api(11, &Args{ .owner = owner, .input = input }, output);
}
pub fn submitColor(owner: buffers.Owner, input: *const abi.GfxModeColorRequest, confirmation_ms: u32, output: *abi.GfxModeStatus) i32 {
    return r4os_rust_gfx_output_api(12, &Args{ .owner = owner, .input = input, .value = confirmation_ms }, output);
}
pub fn modeStatus(ticket: u64, output: *abi.GfxModeStatus) callconv(.c) i32 {
    return r4os_rust_gfx_output_api(13, &Args{ .ticket = ticket }, output);
}
pub fn resolve(owner: buffers.Owner, ticket: u64, action: u32, output: *abi.GfxModeStatus) i32 {
    return r4os_rust_gfx_output_api(14, &Args{ .owner = owner, .ticket = ticket, .value = action }, output);
}
pub fn brightness(input: *const abi.GfxOutputId, output: *abi.GfxOutputBrightness) callconv(.c) i32 {
    return r4os_rust_gfx_output_api(15, &Args{ .input = input }, output);
}
pub fn requestBrightness(owner: buffers.Owner, input: *const abi.GfxBrightnessRequest, output: *abi.GfxBrightnessRequest) i32 {
    return r4os_rust_gfx_output_api(16, &Args{ .owner = owner, .input = input }, output);
}
fn privateCode(err: anyerror) u32 {
    return switch (err) {
        error.Invalid => 1,
        error.Overflow => 2,
        error.Unsupported => 3,
        error.Exhausted => 4,
        error.Budget => 5,
        error.Capacity => 6,
        error.Stale => 7,
        error.Busy => 8,
        error.Closed => 9,
        error.WrongOwner => 10,
        error.OutOfMemory => 11,
        error.Unavailable => 12,
        error.WaitTimeout => 13,
        error.WaitCancelled => 14,
        error.Disabled => 15,
        error.AlreadyCompleted => 16,
        error.Bandwidth => 17,
        error.Routing => 18,
        error.Dependency => 19,
        error.DeviceLost => 20,
        error.RestoreFailed => 21,
        else => unreachable,
    };
}
comptime {
    if (@sizeOf(Args) != 48 or @offsetOf(Args, "input") != 40) @compileError("Output API C args layout");
}
