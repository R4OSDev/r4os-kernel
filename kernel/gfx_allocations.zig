const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;

pub const Error = buffers.Error || error{ Unavailable, WaitTimeout, WaitCancelled };
extern fn r4os_rust_gfx_allocations_run(u32, *const Args, ?*anyopaque) i32;
extern fn r4os_rust_gfx_allocation_receive(*const Args, *abi.GfxBufferReference) i32;
fn call(op: u32, args: Args, out: ?*anyopaque) Error!void {
    return switch (r4os_rust_gfx_allocations_run(op, &args, out)) {
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
        12 => error.Unavailable,
        13 => error.WaitTimeout,
        14 => error.WaitCancelled,
        else => unreachable,
    };
}
pub fn errorCode(err: Error) i32 {
    return switch (err) {
        error.Unavailable => abi.gfx_buffer_error_unavailable,
        error.WaitTimeout => abi.gfx_queue_error_wait_timeout,
        error.WaitCancelled => abi.gfx_queue_error_wait_cancelled,
        else => |other| @import("../program/gfx_buffer_api.zig").status(other),
    };
}
pub fn register(owner: buffers.Owner, config: abi.GfxNativeProvider) Error!buffers.Handle {
    var out: buffers.Handle = undefined;
    try call(0, .{ .owner = owner, .input = &config }, &out);
    return out;
}
pub fn unregister(owner: buffers.Owner, handle: buffers.Handle) Error!void {
    return call(1, .{ .owner = owner, .handle = handle }, null);
}
pub fn closingDriver(id: u32) void {
    call(2, .{ .value = id }, null) catch unreachable;
}
pub fn retainsDriver(id: u32) bool {
    var out: bool = undefined;
    call(3, .{ .value = id }, &out) catch unreachable;
    return out;
}
pub fn start(owner: buffers.Owner, input: abi.GfxNativeAllocation) Error!abi.GfxNativeStatus {
    var out: abi.GfxNativeStatus = undefined;
    try call(4, .{ .owner = owner, .input = &input }, &out);
    return out;
}
pub fn query(owner: buffers.Owner, handle: buffers.Handle) Error!abi.GfxNativeStatus {
    var out: abi.GfxNativeStatus = undefined;
    try call(5, .{ .owner = owner, .handle = handle }, &out);
    return out;
}
pub fn close(owner: buffers.Owner, handle: buffers.Handle) Error!void {
    return call(6, .{ .owner = owner, .handle = handle }, null);
}
pub fn stopped(owner: buffers.Owner) void {
    call(7, .{ .owner = owner }, null) catch unreachable;
}
pub fn take(owner: buffers.Owner, provider: buffers.Handle) Error!abi.GfxNativeJob {
    var out: abi.GfxNativeJob = undefined;
    try call(8, .{ .owner = owner, .handle = provider }, &out);
    return out;
}
pub fn complete(owner: buffers.Owner, provider: buffers.Handle, handle: buffers.Handle, result: i32, reference: abi.GfxBufferHandle) Error!void {
    return call(9, .{ .owner = owner, .handle = provider, .input = &handle, .access = @bitCast(result), .other_input = &reference }, null);
}
pub fn retiredTask(id: u32, generation: u64) void {
    call(10, .{ .value = id, .offset = generation }, null) catch unreachable;
}
pub fn wait(owner: buffers.Owner, handle: buffers.Handle, timeout: u64) Error!abi.GfxNativeStatus {
    var out: abi.GfxNativeStatus = undefined;
    try call(11, .{ .owner = owner, .handle = handle, .value = timeout }, &out);
    return out;
}
pub fn service() bool {
    var out: bool = undefined;
    call(12, .{}, &out) catch unreachable;
    return out;
}
pub fn receive(owner: buffers.Owner, handle: buffers.Handle, output: *abi.GfxBufferReference) i32 {
    const args = Args{ .owner = owner, .handle = handle };
    return r4os_rust_gfx_allocation_receive(&args, output);
}
// The display queue retains its one active Zig owner until .83.23.
export fn r4os_zig_gfx_queue_peer(operation: u32) callconv(.c) bool {
    return switch (operation) {
        0 => @import("../display/queue.zig").available(),
        1 => blk: {
            @import("../display/queue.zig").wake();
            break :blk true;
        },
        else => unreachable,
    };
}
