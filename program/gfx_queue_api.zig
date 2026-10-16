//! Temporary fixed projection of the native public queue API.
pub const abi = @import("r4os_kernel_contract");
const runtime = @import("../display/queue.zig");
const model = runtime.model;
const buffers = @import("../memory/gfx_buffers.zig");
const Args = extern struct { owner: buffers.Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, value: u64 = 0, which: u32 = 0, other: u32 = 0, input: ?*const anyopaque = null, queue: ?*const abi.GfxQueueHandle = null, extra: ?*const anyopaque = null };
extern fn r4os_rust_gfx_queue_api(u32, *const Args, ?*anyopaque) i32;
pub fn errorCode(err: runtime.Error) i32 {
    return r4os_rust_gfx_queue_api(14, &Args{ .which = privateCode(err) }, null);
}
pub fn fence(input: abi.GfxFence) model.Fence {
    var out: model.Fence = undefined;
    _ = r4os_rust_gfx_queue_api(11, &Args{ .input = &input }, &out);
    return out;
}
pub fn publicFence(value: model.Fence) abi.GfxFence {
    var out: abi.GfxFence = undefined;
    _ = r4os_rust_gfx_queue_api(12, &Args{ .input = &value }, &out);
    return out;
}
pub fn publicStatus(value: model.Status) abi.GfxFenceStatus {
    var out: abi.GfxFenceStatus = undefined;
    _ = r4os_rust_gfx_queue_api(13, &Args{ .input = &value }, &out);
    return out;
}
pub fn backend(index: u32, output: *abi.GfxBackendBinding) callconv(.c) i32 {
    return r4os_rust_gfx_queue_api(0, &Args{ .which = index }, output);
}
pub fn backendInfo(index: u32, output: *abi.GfxBackendInfo) callconv(.c) i32 {
    return r4os_rust_gfx_queue_api(1, &Args{ .which = index }, output);
}
pub fn backendProperties(input: *const abi.GfxBackendBinding, output: *abi.GfxBackendProperties) callconv(.c) i32 {
    return r4os_rust_gfx_queue_api(2, &Args{ .input = input }, output);
}
pub fn open(owner: buffers.Owner, input: *const abi.GfxQueueConfig, output: *abi.GfxQueueHandle) i32 {
    return r4os_rust_gfx_queue_api(3, &Args{ .owner = owner, .input = input }, output);
}
pub fn close(owner: buffers.Owner, input: *const abi.GfxQueueHandle) i32 {
    return r4os_rust_gfx_queue_api(4, &Args{ .owner = owner, .input = input }, null);
}
pub fn submit(owner: buffers.Owner, queue_ptr: *const abi.GfxQueueHandle, input: *const abi.GfxSubmission, output: *abi.GfxFenceStatus) i32 {
    return r4os_rust_gfx_queue_api(5, &Args{ .owner = owner, .queue = queue_ptr, .input = input }, output);
}
pub fn submitNative(owner: buffers.Owner, queue_ptr: *const abi.GfxQueueHandle, input: *const abi.GfxSubmission, native_ptr: *const abi.GfxNativeSubmission, output: *abi.GfxFenceStatus) i32 {
    return r4os_rust_gfx_queue_api(6, &Args{ .owner = owner, .queue = queue_ptr, .input = input, .extra = native_ptr }, output);
}
pub fn submitRenderList(owner: buffers.Owner, queue_ptr: *const abi.GfxQueueHandle, input: *const abi.GfxSubmission, list_ptr: *const abi.GfxRenderList, output: *abi.GfxFenceStatus) i32 {
    return r4os_rust_gfx_queue_api(5, &Args{ .owner = owner, .queue = queue_ptr, .input = input, .which = 1, .extra = list_ptr }, output);
}
pub fn submitRenderGridList(owner: buffers.Owner, queue_ptr: *const abi.GfxQueueHandle, input: *const abi.GfxSubmission, list_ptr: *const abi.GfxRenderGridList, output: *abi.GfxFenceStatus) i32 {
    return r4os_rust_gfx_queue_api(5, &Args{ .owner = owner, .queue = queue_ptr, .input = input, .which = 2, .extra = list_ptr }, output);
}
pub fn submitRenderColorList(owner: buffers.Owner, queue_ptr: *const abi.GfxQueueHandle, input: *const abi.GfxSubmission, list_ptr: *const abi.GfxRenderColorList, output: *abi.GfxFenceStatus) i32 {
    return r4os_rust_gfx_queue_api(5, &Args{ .owner = owner, .queue = queue_ptr, .input = input, .which = 3, .extra = list_ptr }, output);
}
pub fn submitRenderColorGridList(owner: buffers.Owner, queue_ptr: *const abi.GfxQueueHandle, input: *const abi.GfxSubmission, list_ptr: *const abi.GfxRenderColorGridList, output: *abi.GfxFenceStatus) i32 {
    return r4os_rust_gfx_queue_api(5, &Args{ .owner = owner, .queue = queue_ptr, .input = input, .which = 4, .extra = list_ptr }, output);
}
pub fn submitOutput(owner: buffers.Owner, queue_ptr: *const abi.GfxQueueHandle, input: *const abi.GfxSubmission, target_ptr: *const abi.GfxOutputTarget, output: *abi.GfxFenceStatus) i32 {
    return r4os_rust_gfx_queue_api(5, &Args{ .owner = owner, .queue = queue_ptr, .input = input, .which = 5, .extra = target_ptr }, output);
}
pub fn query(input: *const abi.GfxFence, output: *abi.GfxFenceStatus) callconv(.c) i32 {
    return r4os_rust_gfx_queue_api(7, &Args{ .input = input }, output);
}
pub fn wait(input: *const abi.GfxFence, timeout_ticks: u64, wait_for: u32, output: *abi.GfxFenceStatus) callconv(.c) i32 {
    return r4os_rust_gfx_queue_api(8, &Args{ .input = input, .value = timeout_ticks, .which = wait_for }, output);
}
pub fn cancel(owner: buffers.Owner, input: *const abi.GfxFence) i32 {
    return r4os_rust_gfx_queue_api(9, &Args{ .owner = owner, .input = input }, null);
}
pub fn release(owner: buffers.Owner, input: *const abi.GfxFence) i32 {
    return r4os_rust_gfx_queue_api(10, &Args{ .owner = owner, .input = input }, null);
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
    if (@sizeOf(Args) != 64 or @offsetOf(Args, "input") != 40) @compileError("Queue API C args layout");
}
