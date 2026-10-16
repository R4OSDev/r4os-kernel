// Typed private projection into the native R4DRAW memory API.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const Args = buffers.lifetime.NativeArgs;
extern fn r4os_rust_gfx_buffer_api(u32, *const Args, ?*anyopaque) i32;
extern fn r4os_rust_gfx_buffer_projection(u32, *const Args, *anyopaque) i32;
extern fn r4os_rust_gfx_buffer_status(i32) i32;
extern fn r4os_rust_gfx_buffer_valid_header(?*const anyopaque, usize) bool;
fn projection(op: u32, args: Args, out: *anyopaque) buffers.Error!void {
    return switch (r4os_rust_gfx_buffer_projection(op, &args, out)) {
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
pub fn status(err: buffers.Error) i32 {
    return r4os_rust_gfx_buffer_status(switch (err) {
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
    });
}
pub fn handle(value: abi.GfxBufferHandle) buffers.Error!buffers.Handle {
    var out: buffers.Handle = undefined;
    try projection(0, .{ .input = &value }, &out);
    return out;
}
pub fn publicHandle(value: buffers.Handle) abi.GfxBufferHandle {
    return .{ .id = value.id, .generation = value.generation };
}
pub fn validOutput(comptime T: type, output: *T) bool {
    return r4os_rust_gfx_buffer_valid_header(output, @sizeOf(T));
}
pub fn referenceLocked(reference: buffers.Handle, owner: buffers.Owner) buffers.Error!abi.GfxBufferReference {
    var out: abi.GfxBufferReference = undefined;
    try projection(3, .{ .handle = reference, .owner = owner }, &out);
    return out;
}
pub fn descriptor(value: abi.GfxBufferDescriptor) buffers.Error!buffers.layout.Descriptor {
    var out: buffers.layout.Descriptor = undefined;
    try projection(1, .{ .input = &value }, &out);
    return out;
}
fn publicDescriptor(value: buffers.layout.Descriptor) abi.GfxBufferDescriptor {
    var out: abi.GfxBufferDescriptor = undefined;
    projection(2, .{ .input = &value }, &out) catch unreachable;
    return out;
}
fn call(op: u32, args: Args, out: ?*anyopaque) i32 {
    return r4os_rust_gfx_buffer_api(op, &args, out);
}
pub fn create(owner: buffers.Owner, input: *const abi.GfxBufferDescriptor, output: *abi.GfxBufferReference) i32 {
    return call(0, .{ .owner = owner, .input = input }, output);
}
pub fn describe(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxBufferDescriptor) i32 {
    return call(1, .{ .owner = owner, .input = input }, output);
}
pub fn import(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxBufferReference) i32 {
    return call(2, .{ .owner = owner, .input = input }, output);
}
pub fn release(owner: buffers.Owner, input: *const abi.GfxBufferHandle) i32 {
    return call(3, .{ .owner = owner, .input = input }, null);
}
pub fn map(owner: buffers.Owner, input: *const abi.GfxBufferHandle, access: u32, offset: u64, bytes: u64, output: *abi.GfxBufferMap) i32 {
    return call(4, .{ .owner = owner, .input = input, .access = access, .offset = offset, .bytes = bytes }, output);
}
pub fn mapPersistent(owner: buffers.Owner, input: *const abi.GfxBufferHandle, access: u32, offset: u64, bytes: u64, output: *abi.GfxBufferMap) i32 {
    return call(5, .{ .owner = owner, .input = input, .access = access, .offset = offset, .bytes = bytes }, output);
}
pub fn unmap(owner: buffers.Owner, input: *const abi.GfxBufferHandle) i32 {
    return call(6, .{ .owner = owner, .input = input }, null);
}
pub fn stats(output: *abi.GfxBufferStats) callconv(.c) i32 {
    return call(7, .{}, output);
}

test "BO public layout preserves checked planes and rejects unknown formats" {
    var value = abi.GfxBufferDescriptor{ .byte_length = 4096, .format = abi.gfx_buffer_format_xrgb8888, .width = 16, .height = 16, .plane_count = 1 };
    value.plane_pitches[0] = 64;
    const native = try descriptor(value);
    try std.testing.expectEqual(buffers.layout.Format.xrgb8888, native.format);
    try std.testing.expectEqualDeep(value, publicDescriptor(native));
    value.format = 0xFFFF_FFFF;
    try std.testing.expectError(error.Unsupported, descriptor(value));
}
