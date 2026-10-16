//! Authenticated DriverApi inputs project to the sole native queue adapter.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = extern struct { owner: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 }, value: u64 = 0, extra_value: u64 = 0, id: u32 = 0, which: u32 = 0, other: u32 = 0, input: ?*const anyopaque = null, second: ?*const anyopaque = null };
extern fn r4os_rust_gfx_driver_queue(u32, *const Args, ?*anyopaque) i32;
pub fn register(identity: buffers.Owner, input: *const abi.GfxBackendRegistration, output: *abi.GfxBackendBinding) i32 {
    const profile = abi.GfxBackendProfile{};
    return r4os_rust_gfx_driver_queue(0, &Args{ .owner = identity, .input = input, .second = &profile }, output);
}
pub fn registerProfile(identity: buffers.Owner, input: *const abi.GfxBackendRegistration, profile: *const abi.GfxBackendProfile, output: *abi.GfxBackendBinding) i32 {
    return r4os_rust_gfx_driver_queue(0, &Args{ .owner = identity, .input = input, .second = profile }, output);
}
pub fn unregister(id: u32, input: *const abi.GfxBackendBinding, quiesced: u32) i32 {
    return r4os_rust_gfx_driver_queue(1, &Args{ .id = id, .input = input, .other = quiesced }, null);
}
pub fn publishProperties(identity: buffers.Owner, input: *const abi.GfxBackendBinding, properties: *const abi.GfxBackendProperties) i32 {
    return r4os_rust_gfx_driver_queue(2, &Args{ .owner = identity, .input = input, .second = properties }, null);
}
pub fn updateOperations(id: u32, input: *const abi.GfxBackendBinding, operations: u64) i32 {
    return r4os_rust_gfx_driver_queue(3, &Args{ .id = id, .input = input, .value = operations }, null);
}
pub fn take(id: u32, input: *const abi.GfxBackendBinding, output: *abi.GfxDriverJob) i32 {
    return r4os_rust_gfx_driver_queue(4, &Args{ .id = id, .input = input }, output);
}
pub fn complete(id: u32, input: *const abi.GfxFence, result: u32, quiesced: u32) i32 {
    return r4os_rust_gfx_driver_queue(5, &Args{ .id = id, .input = input, .which = result, .other = quiesced }, null);
}
pub fn readRenderList(id: u32, input: *const abi.GfxFence, output: *abi.GfxRenderList) i32 {
    return r4os_rust_gfx_driver_queue(6, &Args{ .id = id, .input = input }, output);
}
pub fn readRenderGridList(id: u32, input: *const abi.GfxFence, output: *abi.GfxRenderGridList) i32 {
    return r4os_rust_gfx_driver_queue(7, &Args{ .id = id, .input = input }, output);
}
pub fn reset(id: u32, input: *const abi.GfxBackendBinding, quiesced: u32, output: *abi.GfxBackendBinding) i32 {
    return r4os_rust_gfx_driver_queue(8, &Args{ .id = id, .input = input, .other = quiesced }, output);
}
pub fn readRenderColorList(id: u32, input: *const abi.GfxFence, output: *abi.GfxRenderColorList) i32 {
    return r4os_rust_gfx_driver_queue(9, &Args{ .id = id, .input = input }, output);
}
pub fn segment(id: u32, input: *const abi.GfxFence, which: u32, offset: u64, mask: u64, output: *abi.GfxDmaSegment) i32 {
    return r4os_rust_gfx_driver_queue(10, &Args{ .id = id, .input = input, .which = which, .value = offset, .extra_value = mask }, output);
}
pub fn retain(identity: buffers.Owner, input: *const abi.GfxFence, which: u32, output: *abi.GfxBufferReference) i32 {
    return r4os_rust_gfx_driver_queue(11, &Args{ .owner = identity, .input = input, .which = which }, output);
}
pub fn retainScanout(identity: buffers.Owner, input: *const abi.GfxFence, output: *abi.GfxBufferReference) i32 {
    return r4os_rust_gfx_driver_queue(12, &Args{ .owner = identity, .input = input }, output);
}
pub fn readNativeInfo(id: u32, input: *const abi.GfxFence, output: *abi.GfxNativeJobInfo) i32 {
    return r4os_rust_gfx_driver_queue(13, &Args{ .id = id, .input = input }, output);
}
pub fn queueOwnerInfo(id: u32, input: *const abi.GfxBackendBinding, timeline: u64, output: *abi.GfxQueueOwnerInfo) i32 {
    return r4os_rust_gfx_driver_queue(14, &Args{ .id = id, .input = input, .value = timeline }, output);
}
pub fn readNativeData(id: u32, input: *const abi.GfxFence, offset: u32, output: [*]u8, count: u32) i32 {
    return r4os_rust_gfx_driver_queue(15, &Args{ .id = id, .input = input, .which = offset, .other = count }, output);
}
pub fn readNativeBinding(id: u32, input: *const abi.GfxFence, index: u32, output: *abi.GfxNativeBinding) i32 {
    return r4os_rust_gfx_driver_queue(16, &Args{ .id = id, .input = input, .which = index }, output);
}
pub fn beginScanout(id: u32, input: *const abi.GfxFence) i32 {
    return r4os_rust_gfx_driver_queue(17, &Args{ .id = id, .input = input }, null);
}
pub fn scanoutRetireRequested(id: u32, input: *const abi.GfxFence) i32 {
    return r4os_rust_gfx_driver_queue(18, &Args{ .id = id, .input = input }, null);
}
comptime {
    if (@sizeOf(Args) != 72 or @offsetOf(Args, "input") != 56) @compileError("Driver queue API C args layout");
}
