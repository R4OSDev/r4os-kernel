const abi = @import("r4os_kernel_contract");
const api = @import("../program/gfx_buffer_api.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const runtime = @import("gfx_allocations.zig");

pub fn register(owner: buffers.Owner, input: *const abi.GfxNativeProvider, output: *abi.GfxBufferHandle) i32 {
    if (@intFromPtr(input) == 0 or @intFromPtr(output) == 0) return abi.gfx_buffer_error_invalid;
    output.* = api.publicHandle(runtime.register(owner, input.*) catch |err| return runtime.errorCode(err));
    return 1;
}
pub fn unregister(owner: buffers.Owner, input: *const abi.GfxBufferHandle) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    runtime.unregister(owner, handle) catch |err| return runtime.errorCode(err);
    return 1;
}
pub fn take(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxNativeJob) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxNativeJob, output)) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    output.* = runtime.take(owner, handle) catch |err| return runtime.errorCode(err);
    return 1;
}
pub fn complete(owner: buffers.Owner, provider: *const abi.GfxBufferHandle, request: *const abi.GfxBufferHandle, result: i32, reference: *const abi.GfxBufferHandle) i32 {
    if (@intFromPtr(provider) == 0 or @intFromPtr(request) == 0 or @intFromPtr(reference) == 0) return abi.gfx_buffer_error_invalid;
    const provider_handle = api.handle(provider.*) catch |err| return api.status(err);
    const request_handle = api.handle(request.*) catch |err| return api.status(err);
    runtime.complete(owner, provider_handle, request_handle, result, reference.*) catch |err| return runtime.errorCode(err);
    return 1;
}
