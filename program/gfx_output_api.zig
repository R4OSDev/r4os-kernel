// Public copied-payload facade; outputs are written only after owner release.
const outputs = @import("../display/outputs.zig");
pub const abi = outputs.abi;
const buffers = @import("../memory/gfx_buffers.zig");
const memory_api = @import("gfx_buffer_api.zig");
const queue_api = @import("gfx_queue_api.zig");
const lifetime = @import("../sched/task_context.zig");
pub fn code(err: outputs.Error) i32 {
    return switch (err) {
        error.Bandwidth => abi.gfx_output_error_bandwidth,
        error.Routing => abi.gfx_output_error_routing,
        error.Dependency => abi.gfx_output_error_dependency,
        else => |other| queue_api.errorCode(other),
    };
}
pub fn revision(output: *abi.GfxDisplayRevision) callconv(.c) i32 {
    if (!memory_api.validOutput(abi.GfxDisplayRevision, output)) return abi.gfx_output_error_invalid;
    const snapshot = outputs.revision();
    output.* = snapshot;
    return abi.gfx_output_ok;
}
pub fn info(index: u32, output: *abi.GfxOutputInfo) callconv(.c) i32 {
    if (!memory_api.validOutput(abi.GfxOutputInfo, output)) return abi.gfx_output_error_invalid;
    const snapshot = outputs.infoAt(index) orelse return 0;
    output.* = snapshot;
    return abi.gfx_output_ok;
}
pub fn mode(input: *const abi.GfxOutputId, index: u32, output: *abi.GfxOutputMode) callconv(.c) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxOutputMode, output)) return abi.gfx_output_error_invalid;
    const snapshot = outputs.modeAt(input.*, index) catch |err| return code(err);
    output.* = snapshot orelse return 0;
    return abi.gfx_output_ok;
}
pub fn edid(input: *const abi.GfxOutputId, index: u32, output: *abi.GfxEdidBlock) callconv(.c) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxEdidBlock, output)) return abi.gfx_output_error_invalid;
    const snapshot = outputs.edidAt(input.*, index) catch |err| return code(err);
    output.* = snapshot orelse return 0;
    return abi.gfx_output_ok;
}
pub fn atomic(owner: buffers.Owner, input: *const abi.GfxAtomicState, output: *abi.GfxAtomicResult, commit: bool) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxAtomicResult, output)) return abi.gfx_output_error_invalid;
    const state = input.*;
    const call = lifetime.enterUnwind();
    if (!call.admitted()) return abi.gfx_output_error_busy;
    defer _ = lifetime.leaveUnwind(call);
    const result = outputs.atomic(owner, &state, commit) catch |err| return code(err);
    output.* = result;
    return abi.gfx_output_ok;
}
const modes = @import("../display/mode_work.zig");
pub fn submit(owner: buffers.Owner, input: *const abi.GfxAtomicState, confirmation_ms: u32, output: *abi.GfxModeStatus) i32 {
    if (@intFromPtr(input) == 0 or !memory_api.validOutput(abi.GfxModeStatus, output)) return abi.gfx_output_error_invalid;
    const state = input.*;
    const call = lifetime.enterUnwind();
    if (!call.admitted()) return abi.gfx_output_error_busy;
    defer _ = lifetime.leaveUnwind(call);
    const result = modes.submit(owner, &state, confirmation_ms) catch |err| return modes.code(err);
    output.* = result;
    return abi.gfx_output_ok;
}
pub fn modeStatus(ticket: u64, output: *abi.GfxModeStatus) callconv(.c) i32 {
    if (!memory_api.validOutput(abi.GfxModeStatus, output)) return abi.gfx_output_error_invalid;
    const result = modes.status(ticket) catch |err| return modes.code(err);
    output.* = result;
    return abi.gfx_output_ok;
}
pub fn resolve(owner: buffers.Owner, ticket: u64, action: u32, output: *abi.GfxModeStatus) i32 {
    if (!memory_api.validOutput(abi.GfxModeStatus, output)) return abi.gfx_output_error_invalid;
    const result = modes.resolve(owner, ticket, action) catch |err| return modes.code(err);
    output.* = result;
    return abi.gfx_output_ok;
}
