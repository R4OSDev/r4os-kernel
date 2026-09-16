pub const abi = @import("r4os_kernel_contract");
const api = @import("gfx_buffer_api.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const runtime = @import("../kernel/gfx_virtual.zig");
const context = @import("../sched/task_context.zig");

pub fn start(owner: buffers.Owner, closed: *const bool, input: *const abi.GfxVirtualRequest, output: *abi.GfxVirtualStatus) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxVirtualStatus, output)) return abi.gfx_buffer_error_invalid;
    const value = input.*;
    const guard = context.enterUnwind();
    if (!guard.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(guard);
    output.* = runtime.start(owner, closed, value) catch |err| return runtime.errorCode(err);
    return abi.gfx_buffer_result_ok;
}
pub fn query(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxVirtualStatus) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxVirtualStatus, output)) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    output.* = runtime.query(owner, handle) catch |err| return runtime.errorCode(err);
    return abi.gfx_buffer_result_ok;
}
pub fn close(owner: buffers.Owner, input: *const abi.GfxBufferHandle, mode: u32) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    const guard = context.enterUnwind();
    if (!guard.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(guard);
    runtime.close(owner, handle, mode) catch |err| return runtime.errorCode(err);
    return abi.gfx_buffer_result_ok;
}
pub fn wait(owner: buffers.Owner, input: *const abi.GfxBufferHandle, until: u32, timeout: u64, output: *abi.GfxVirtualStatus) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxVirtualStatus, output)) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    output.* = runtime.wait(owner, handle, until, timeout) catch |err| return runtime.errorCode(err);
    return abi.gfx_buffer_result_ok;
}
