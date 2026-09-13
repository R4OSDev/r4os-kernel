const abi = @import("r4os_kernel_contract");
const api = @import("gfx_buffer_api.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const runtime = @import("../kernel/gfx_allocations.zig");
const context = @import("../sched/task_context.zig");

pub fn start(owner: buffers.Owner, input: *const abi.GfxNativeAllocation, output: *abi.GfxNativeStatus) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxNativeStatus, output)) return abi.gfx_buffer_error_invalid;
    const value = input.*;
    const guard = context.enterUnwind();
    if (!guard.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(guard);
    output.* = runtime.start(owner, value) catch |err| return runtime.errorCode(err);
    return 1;
}
pub fn query(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxNativeStatus) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxNativeStatus, output)) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    output.* = runtime.query(owner, handle) catch |err| return runtime.errorCode(err);
    return 1;
}
pub fn receive(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxBufferReference) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxBufferReference, output)) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    const guard = context.enterUnwind();
    if (!guard.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(guard);
    return runtime.receive(owner, handle, output);
}
pub fn close(owner: buffers.Owner, input: *const abi.GfxBufferHandle) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    const guard = context.enterUnwind();
    if (!guard.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(guard);
    runtime.close(owner, handle) catch |err| return runtime.errorCode(err);
    return 1;
}
pub fn wait(owner: buffers.Owner, input: *const abi.GfxBufferHandle, timeout_ticks: u64, output: *abi.GfxNativeStatus) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxNativeStatus, output)) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    // No unwind hold across blocking wait: exact task retirement owns cleanup.
    output.* = runtime.wait(owner, handle, timeout_ticks) catch |err| return runtime.errorCode(err);
    return 1;
}
