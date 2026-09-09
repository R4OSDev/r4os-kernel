// R4DRAW bridge for the memory owner; no renderer or placement policy here.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const task_context = @import("../sched/task_context.zig");

pub fn status(err: buffers.Error) i32 {
    return switch (err) {
        error.Invalid, error.WrongOwner => abi.gfx_buffer_error_invalid,
        error.Overflow, error.Exhausted => abi.gfx_buffer_error_overflow,
        error.Unsupported => abi.gfx_buffer_error_unsupported,
        error.OutOfMemory => abi.gfx_buffer_error_oom,
        error.Budget => abi.gfx_buffer_error_budget,
        error.Capacity => abi.gfx_buffer_error_capacity,
        error.Stale => abi.gfx_buffer_error_stale,
        error.Busy => abi.gfx_buffer_error_busy,
        error.Closed => abi.gfx_buffer_error_closed,
    };
}
pub fn handle(value: abi.GfxBufferHandle) buffers.Error!buffers.Handle {
    if (value.id == 0 or value.reserved0 != 0 or value.generation == 0) return error.Invalid;
    return .{ .id = value.id, .generation = value.generation };
}
pub fn publicHandle(value: buffers.Handle) abi.GfxBufferHandle {
    return .{ .id = value.id, .generation = value.generation };
}
pub fn validOutput(comptime T: type, output: *T) bool {
    return @intFromPtr(output) != 0 and output.version == 1 and output.size >= @sizeOf(T);
}
pub fn referenceLocked(reference: buffers.Handle, owner: buffers.Owner) buffers.Error!abi.GfxBufferReference {
    return .{
        .buffer = publicHandle(try buffers.store.bufferFor(reference, owner)),
        .reference = publicHandle(reference),
        .flags = if (try buffers.store.readOnly(reference, owner)) abi.gfx_buffer_reference_immutable else 0,
    };
}
fn descriptor(value: abi.GfxBufferDescriptor) buffers.Error!buffers.layout.Descriptor {
    if (value.version != 1 or value.size < @sizeOf(abi.GfxBufferDescriptor) or value.reserved0 != 0) return error.Invalid;
    var result = buffers.layout.Descriptor{
        .bytes = value.byte_length,
        .alignment = value.alignment,
        .modifier = value.modifier,
        .width = value.width,
        .height = value.height,
        .format = std.enums.fromInt(buffers.layout.Format, value.format) orelse return error.Unsupported,
        .plane_count = value.plane_count,
        .usage = value.usage,
        .location = std.enums.fromInt(buffers.layout.Location, value.location) orelse return error.Unsupported,
        .binding = .{ .adapter = value.adapter_id, .driver_owner = value.driver_owner, .device_generation = value.device_generation },
    };
    for (&result.planes, 0..) |*plane, index| plane.* = .{ .offset = value.plane_offsets[index], .pitch = value.plane_pitches[index] };
    return result;
}
fn publicDescriptor(value: buffers.layout.Descriptor) abi.GfxBufferDescriptor {
    var result = abi.GfxBufferDescriptor{
        .byte_length = value.bytes,
        .alignment = value.alignment,
        .modifier = value.modifier,
        .width = value.width,
        .height = value.height,
        .format = @intFromEnum(value.format),
        .plane_count = value.plane_count,
        .usage = value.usage,
        .location = @intFromEnum(value.location),
        .adapter_id = value.binding.adapter,
        .driver_owner = value.binding.driver_owner,
        .device_generation = value.binding.device_generation,
    };
    for (value.planes, 0..) |plane, index| {
        result.plane_offsets[index] = plane.offset;
        result.plane_pitches[index] = plane.pitch;
    }
    return result;
}

pub fn create(owner: buffers.Owner, input: *const abi.GfxBufferDescriptor, output: *abi.GfxBufferReference) i32 {
    if (@intFromPtr(input) == 0 or !validOutput(abi.GfxBufferReference, output)) return abi.gfx_buffer_error_invalid;
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = task_context.leaveUnwind(call);
    const desc = descriptor(input.*) catch |err| return status(err);
    const created = buffers.create(owner, desc) catch |err| return status(err);
    output.* = .{ .buffer = publicHandle(created.buffer), .reference = publicHandle(created.reference) };
    return abi.gfx_buffer_result_ok;
}
pub fn describe(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxBufferDescriptor) i32 {
    if (@intFromPtr(input) == 0 or !validOutput(abi.GfxBufferDescriptor, output)) return abi.gfx_buffer_error_invalid;
    const reference = handle(input.*) catch |err| return status(err);
    const value = blk: {
        buffers.lock();
        defer buffers.unlock();
        break :blk publicDescriptor(buffers.store.describe(reference, owner) catch |err| return status(err));
    };
    output.* = value;
    return abi.gfx_buffer_result_ok;
}
pub fn import(owner: buffers.Owner, input: *const abi.GfxBufferHandle, output: *abi.GfxBufferReference) i32 {
    if (@intFromPtr(input) == 0 or !validOutput(abi.GfxBufferReference, output)) return abi.gfx_buffer_error_invalid;
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = task_context.leaveUnwind(call);
    const source = handle(input.*) catch |err| return status(err);
    const value = blk: {
        buffers.lock();
        defer buffers.unlock();
        const reference = buffers.store.share(source, owner) catch |err| return status(err);
        break :blk referenceLocked(reference, owner) catch unreachable;
    };
    output.* = value;
    return abi.gfx_buffer_result_ok;
}
pub fn release(owner: buffers.Owner, input: *const abi.GfxBufferHandle) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_buffer_error_invalid;
    const reference = handle(input.*) catch |err| return status(err);
    buffers.drop(reference, owner) catch |err| return status(err);
    return abi.gfx_buffer_result_ok;
}
pub fn map(owner: buffers.Owner, input: *const abi.GfxBufferHandle, access: u32, offset: u64, bytes: u64, output: *abi.GfxBufferMap) i32 {
    if (@intFromPtr(input) == 0 or !validOutput(abi.GfxBufferMap, output) or access > abi.gfx_buffer_map_write) return abi.gfx_buffer_error_invalid;
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = task_context.leaveUnwind(call);
    const reference = handle(input.*) catch |err| return status(err);
    const use = blk: {
        buffers.lock();
        defer buffers.unlock();
        break :blk buffers.mapLocked(reference, owner, if (access == abi.gfx_buffer_map_read) .cpu_read else .cpu_write, offset, bytes) catch |err| return status(err);
    };
    output.* = .{ .lease = publicHandle(use.lease), .cpu_address = use.backing.cpu_address + offset, .byte_length = bytes, .cache_policy = @intFromEnum(use.backing.cache) };
    return abi.gfx_buffer_result_ok;
}
pub fn unmap(owner: buffers.Owner, input: *const abi.GfxBufferHandle) i32 {
    if (@intFromPtr(input) == 0) return abi.gfx_buffer_error_invalid;
    const lease = handle(input.*) catch |err| return status(err);
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = task_context.leaveUnwind(call);
    buffers.lock();
    buffers.unmapCpuLocked(lease, owner) catch |err| {
        buffers.unlock();
        return status(err);
    };
    buffers.unlock();
    buffers.collect();
    return abi.gfx_buffer_result_ok;
}
pub fn stats(output: *abi.GfxBufferStats) callconv(.c) i32 {
    if (!validOutput(abi.GfxBufferStats, output)) return abi.gfx_buffer_error_invalid;
    const value: abi.GfxBufferStats = blk: {
        buffers.lock();
        defer buffers.unlock();
        const snapshot = buffers.store.stats();
        break :blk .{ .objects = @intCast(snapshot.objects), .references = @intCast(snapshot.references), .leases = @intCast(snapshot.leases), .committed_bytes = snapshot.bytes, .retained_bytes = snapshot.retained_bytes, .budget_bytes = buffers.store.budget_bytes, .producer_budget_bytes = buffers.store.producer_budget_bytes };
    };
    output.* = value;
    return abi.gfx_buffer_result_ok;
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
