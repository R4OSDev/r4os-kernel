const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;

extern fn r4os_rust_gfx_virtual_api(bool, u32, *const Args, ?*anyopaque) i32;
fn call(op: u32, args: Args, out: ?*anyopaque) i32 {
    return r4os_rust_gfx_virtual_api(true, op, &args, out);
}
pub fn register(owner: buffers.Owner, input: *const abi.GfxNativeProvider, output: *abi.GfxBufferHandle) i32 {
    return call(0, .{ .owner = owner, .input = input }, output);
}
pub fn unregister(owner: buffers.Owner, input: *const abi.GfxBufferHandle) i32 {
    return call(1, .{ .owner = owner, .input = input }, null);
}
pub fn take(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxVirtualJob) i32 {
    return call(2, .{ .owner = owner, .input = input }, output);
}
pub fn complete(owner: buffers.Owner, input: *const abi.GfxBufferHandle, completion: *const abi.GfxVirtualCompletion) i32 {
    return call(3, .{ .owner = owner, .input = input, .other_input = completion }, null);
}
