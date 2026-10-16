const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;

pub const Error = buffers.Error || error{ Unavailable, WaitTimeout, WaitCancelled };
extern fn r4os_rust_gfx_virtual_run(u32, *const Args, ?*anyopaque) i32;
fn call(op: u32, args: Args, out: ?*anyopaque) Error!void {
    return switch (r4os_rust_gfx_virtual_run(op, &args, out)) {
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
    return @import("gfx_allocations.zig").errorCode(err);
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
pub fn stopAdmission(closed: *bool) void {
    call(4, .{ .buffer = closed }, null) catch unreachable;
}
pub fn start(owner: buffers.Owner, closed: *const bool, input: abi.GfxVirtualRequest) Error!abi.GfxVirtualStatus {
    var out: abi.GfxVirtualStatus = undefined;
    try call(5, .{ .owner = owner, .input = &input, .other_input = closed }, &out);
    return out;
}
pub fn query(owner: buffers.Owner, handle: buffers.Handle) Error!abi.GfxVirtualStatus {
    var out: abi.GfxVirtualStatus = undefined;
    try call(6, .{ .owner = owner, .handle = handle }, &out);
    return out;
}
pub const Execution = extern struct { entry: *@import("../memory/gfx_virtual_state.zig").Entry, driver: buffers.Owner, reference: buffers.Handle, binding: abi.GfxNativeBinding };
pub fn retainExecutionLocked(owner: buffers.Owner, input: abi.GfxNativeResource, target: buffers.layout.Binding) Error!Execution {
    var out: Execution = undefined;
    try call(7, .{ .owner = owner, .input = &input, .binding = target }, &out);
    return out;
}
pub fn releaseExecutionLocked(value: Execution) void {
    call(8, .{ .input = &value }, null) catch unreachable;
}
pub fn close(owner: buffers.Owner, handle: buffers.Handle, mode: u32) Error!void {
    return call(9, .{ .owner = owner, .handle = handle, .access = mode }, null);
}
pub fn stopped(owner: buffers.Owner) void {
    call(10, .{ .owner = owner }, null) catch unreachable;
}
pub fn take(owner: buffers.Owner, handle: buffers.Handle) Error!abi.GfxVirtualJob {
    var out: abi.GfxVirtualJob = undefined;
    try call(11, .{ .owner = owner, .handle = handle }, &out);
    return out;
}
pub fn complete(owner: buffers.Owner, handle: buffers.Handle, completion: abi.GfxVirtualCompletion) Error!void {
    return call(12, .{ .owner = owner, .handle = handle, .input = &completion }, null);
}
pub fn retiredTask(id: u32, generation: u64) void {
    call(13, .{ .value = id, .offset = generation }, null) catch unreachable;
}
pub fn wait(owner: buffers.Owner, handle: buffers.Handle, until: u32, timeout: u64) Error!abi.GfxVirtualStatus {
    var out: abi.GfxVirtualStatus = undefined;
    try call(14, .{ .owner = owner, .handle = handle, .access = until, .value = timeout }, &out);
    return out;
}
pub fn service() bool {
    var out: bool = undefined;
    call(15, .{}, &out) catch unreachable;
    return out;
}
comptime {
    if (@sizeOf(Execution) != 120 or @alignOf(Execution) != 8 or @offsetOf(Execution, "entry") != 0 or @offsetOf(Execution, "driver") != 8 or @offsetOf(Execution, "reference") != 32 or @offsetOf(Execution, "binding") != 48) @compileError("native VA execution record");
}
