const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;

extern fn r4os_rust_gfx_allocation_api(bool, u32, *const Args, ?*anyopaque) i32;
fn call(op: u32, args: Args, out: ?*anyopaque) i32 {
    return r4os_rust_gfx_allocation_api(false, op, &args, out);
}
pub fn start(owner: buffers.Owner, input: *const abi.GfxNativeAllocation, output: *abi.GfxNativeStatus) i32 {
    return call(0, .{ .owner = owner, .input = input }, output);
}
pub fn query(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxNativeStatus) i32 {
    return call(1, .{ .owner = owner, .input = input }, output);
}
pub fn receive(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxBufferReference) i32 {
    return call(2, .{ .owner = owner, .input = input }, output);
}
pub fn close(owner: buffers.Owner, input: *const abi.GfxBufferHandle) i32 {
    return call(3, .{ .owner = owner, .input = input }, null);
}
pub fn wait(owner: buffers.Owner, input: *const abi.GfxBufferHandle, timeout: u64, output: *abi.GfxNativeStatus) i32 {
    return call(4, .{ .owner = owner, .input = input, .value = timeout }, output);
}
