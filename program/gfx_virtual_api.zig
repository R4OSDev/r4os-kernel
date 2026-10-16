pub const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;

extern fn r4os_rust_gfx_virtual_api(bool, u32, *const Args, ?*anyopaque) i32;
fn call(op: u32, args: Args, out: ?*anyopaque) i32 {
    return r4os_rust_gfx_virtual_api(false, op, &args, out);
}
pub fn start(owner: buffers.Owner, closed: *const bool, input: *const abi.GfxVirtualRequest, output: *abi.GfxVirtualStatus) i32 {
    return call(0, .{ .owner = owner, .other_input = closed, .input = input }, output);
}
pub fn query(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxVirtualStatus) i32 {
    return call(1, .{ .owner = owner, .input = input }, output);
}
pub fn close(owner: buffers.Owner, input: *const abi.GfxBufferHandle, mode: u32) i32 {
    return call(2, .{ .owner = owner, .input = input, .access = mode }, null);
}
pub fn wait(owner: buffers.Owner, input: *const abi.GfxBufferHandle, until: u32, timeout: u64, output: *abi.GfxVirtualStatus) i32 {
    return call(3, .{ .owner = owner, .input = input, .access = until, .value = timeout }, output);
}
