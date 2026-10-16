// Typed C projection of native driver epochs/device/MMIO runtime.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;
pub const Owner = buffers.Owner;
extern fn r4os_rust_gfx_driver_memory_run(u32, *const Args, ?*anyopaque) i32;
fn call(op: u32, args: Args, out: ?*anyopaque) i32 {
    return r4os_rust_gfx_driver_memory_run(op, &args, out);
}
fn checked(value: i32) buffers.Error!void {
    return switch (value) {
        0 => {},
        1 => error.Invalid,
        2 => error.Overflow,
        3 => error.Unsupported,
        4 => error.Exhausted,
        5 => error.Budget,
        6 => error.Capacity,
        7 => error.Stale,
        8 => error.Busy,
        9 => error.Closed,
        10 => error.WrongOwner,
        11 => error.OutOfMemory,
        else => unreachable,
    };
}
pub fn bind(id: u32, generation: u64) bool {
    var out: bool = undefined;
    checked(call(0, .{ .value = id, .offset = generation }, &out)) catch unreachable;
    return out;
}
pub fn discardEmptyBinding(id: u32, generation: u64) bool {
    var out: bool = undefined;
    checked(call(1, .{ .value = id, .offset = generation }, &out)) catch unreachable;
    return out;
}
pub fn owner(id: u32, admission: bool) buffers.Error!Owner {
    var out: Owner = undefined;
    try checked(call(2, .{ .value = id, .flag = admission }, &out));
    return out;
}
pub fn admitLocked(identity: Owner) buffers.Error!void {
    return checked(call(3, .{ .owner = identity }, null));
}
pub fn beginClose(id: u32) void {
    checked(call(4, .{ .value = id }, null)) catch unreachable;
}
pub fn retained(id: u32) bool {
    var out: bool = undefined;
    checked(call(5, .{ .value = id }, &out)) catch unreachable;
    return out;
}
pub fn finishOwner(id: u32) void {
    checked(call(6, .{ .value = id }, null)) catch unreachable;
}
pub fn acquire(identity: Owner, reference: *const abi.GfxBufferHandle, request: *const abi.GfxDeviceRequest, output: *abi.GfxDeviceLease) i32 {
    return call(7, .{ .owner = identity, .input = reference, .other_input = request }, output);
}
pub fn segment(identity: Owner, input: *const abi.GfxDeviceLease, offset: u64, output: *abi.GfxDmaSegment) i32 {
    return call(8, .{ .owner = identity, .input = input, .offset = offset }, output);
}
pub fn release(identity: Owner, input: *const abi.GfxDeviceLease, quiesced: u32) i32 {
    return call(9, .{ .owner = identity, .input = input, .value = quiesced }, null);
}
pub fn mapWindow(identity: Owner, input: *const abi.GfxMmioRequest, output: *abi.GfxMmioWindow) i32 {
    return call(10, .{ .owner = identity, .input = input }, output);
}
pub fn unmapWindow(identity: Owner, input: *const abi.GfxBufferHandle, quiesced: u32) i32 {
    return call(11, .{ .owner = identity, .input = input, .value = quiesced }, null);
}
pub fn collect(identity: Owner) i32 {
    return call(12, .{ .owner = identity }, null);
}
pub fn collectBuffers(identity: Owner) i32 {
    return call(13, .{ .owner = identity }, null);
}
pub fn dmaSegment(value: buffers.lifetime.Use, offset: u64, mask: u64) buffers.Error!abi.GfxDmaSegment {
    const wire = buffers.lifetime.NativeUse.from(value);
    var out: abi.GfxDmaSegment = undefined;
    try checked(call(14, .{ .input = &wire, .offset = offset, .value = mask }, &out));
    return out;
}
