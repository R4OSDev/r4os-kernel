const abi = @import("r4os_kernel_contract");
const api = @import("../program/gfx_buffer_api.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const runtime = @import("gfx_virtual.zig");

pub fn register(owner: buffers.Owner, input: *const abi.GfxNativeProvider, output: *abi.GfxBufferHandle) i32 {
    if (@intFromPtr(input) == 0 or @intFromPtr(output) == 0) return abi.gfx_buffer_error_invalid;
    output.* = api.publicHandle(runtime.register(owner, input.*) catch |err| return runtime.errorCode(err));
    return abi.gfx_buffer_result_ok;
}
pub fn unregister(owner: buffers.Owner, input: *const abi.GfxBufferHandle) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    runtime.unregister(owner, handle) catch |err| return runtime.errorCode(err);
    return abi.gfx_buffer_result_ok;
}
pub fn take(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxVirtualJob) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxVirtualJob, output)) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    output.* = runtime.take(owner, handle) catch |err| return runtime.errorCode(err);
    return abi.gfx_buffer_result_ok;
}
pub fn complete(owner: buffers.Owner, input: *const abi.GfxBufferHandle, completion: *const abi.GfxVirtualCompletion) i32 {
    if (@intFromPtr(input) == 0 or @intFromPtr(completion) == 0) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    runtime.complete(owner, handle, completion.*) catch |err| return runtime.errorCode(err);
    return abi.gfx_buffer_result_ok;
}
