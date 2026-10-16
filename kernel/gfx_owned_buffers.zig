// Typed C projection into the single native metadata owner.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;
const Owner = buffers.Owner;
extern fn r4os_rust_gfx_owned_api(u32, *const Args, ?*anyopaque) i32;
fn call(op: u32, args: Args, out: ?*anyopaque) i32 {
    return r4os_rust_gfx_owned_api(op, &args, out);
}
pub fn lose(identity: Owner, adapter: u32, generation: u64, quiesced: u32) i32 {
    return call(0, .{ .owner = identity, .access = adapter, .offset = generation, .value = quiesced }, null);
}
pub fn reserve(identity: Owner, input: *const abi.GfxBufferDescriptor, cookie: u64, output: *abi.GfxOwnedBufferReservation) i32 {
    return call(1, .{ .owner = identity, .input = input, .value = cookie }, output);
}
pub fn commit(identity: Owner, input: *const abi.GfxOwnedBufferReservation, output: *abi.GfxBufferReference) i32 {
    return call(2, .{ .owner = identity, .input = input }, output);
}
pub fn abort(identity: Owner, input: *const abi.GfxOwnedBufferReservation, quiesced: u32) i32 {
    return call(3, .{ .owner = identity, .input = input, .value = quiesced }, null);
}
pub fn take(identity: Owner, adapter: u32, generation: u64, output: *abi.GfxOwnedBufferRelease) i32 {
    return call(4, .{ .owner = identity, .access = adapter, .offset = generation }, output);
}
pub fn finish(identity: Owner, input: *const abi.GfxOwnedBufferRelease, quiesced: u32) i32 {
    return call(5, .{ .owner = identity, .input = input, .value = quiesced }, null);
}
