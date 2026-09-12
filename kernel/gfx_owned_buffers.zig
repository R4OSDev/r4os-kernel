// Common ownership only. Allocation, GPU mappings and placement stay in R4D.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const api = @import("../program/gfx_buffer_api.zig");
const driver = @import("gfx_driver_memory.zig");
const context = @import("../sched/task_context.zig");
const Owner = buffers.Owner;

fn creation(input: *const abi.GfxOwnedBufferReservation) buffers.Error!buffers.lifetime.OwnedCreate {
    if (@intFromPtr(input) == 0) return error.Invalid;
    const v = input.*;
    if (v.version != 1 or v.size < @sizeOf(abi.GfxOwnedBufferReservation) or v.reserved0 != 0) return error.Invalid;
    return .{ .create = .{ .buffer = try api.handle(v.buffer), .reference = try api.handle(v.reference), .bytes = v.allocation_bytes },
        .cookie = v.cookie, .driver = .{ .kind = .driver, .id = v.driver_owner, .generation = v.driver_generation },
        .binding = .{ .adapter = v.adapter_id, .driver_owner = v.driver_owner, .device_generation = v.device_generation } };
}
pub fn reserve(identity: Owner, input: *const abi.GfxBufferDescriptor, cookie: u64, output: *abi.GfxOwnedBufferReservation) i32 {
    if (@intFromPtr(input) == 0 or !api.validOutput(abi.GfxOwnedBufferReservation, output)) return abi.gfx_buffer_error_invalid;
    var desc = api.descriptor(input.*) catch |err| return api.status(err);
    if (desc.binding.driver_owner != 0 and desc.binding.driver_owner != identity.id) return abi.gfx_buffer_error_invalid;
    desc.binding.driver_owner = @intCast(identity.id);
    const call = context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(call);
    const ticket = blk: {
        buffers.lock();
        defer buffers.unlock();
        driver.admitLocked(identity) catch |err| return api.status(err);
        break :blk buffers.store.beginOwned(identity, desc, cookie) catch |err| return api.status(err);
    };
    output.* = .{ .buffer = api.publicHandle(ticket.create.buffer), .reference = api.publicHandle(ticket.create.reference),
        .allocation_bytes = ticket.create.bytes, .cookie = ticket.cookie, .device_generation = ticket.binding.device_generation,
        .driver_generation = identity.generation, .adapter_id = ticket.binding.adapter, .driver_owner = @intCast(identity.id) };
    return abi.gfx_buffer_result_ok;
}
pub fn commit(identity: Owner, input: *const abi.GfxOwnedBufferReservation, output: *abi.GfxBufferReference) i32 {
    if (!api.validOutput(abi.GfxBufferReference, output)) return abi.gfx_buffer_error_invalid;
    const ticket = creation(input) catch |err| return api.status(err);
    const call = context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(call);
    const reference = blk: {
        buffers.lock();
        defer buffers.unlock();
        driver.admitLocked(identity) catch |err| return api.status(err);
        buffers.store.commitOwned(ticket, identity) catch |err| return api.status(err);
        break :blk api.referenceLocked(ticket.create.reference, identity) catch unreachable;
    };
    output.* = reference;
    return abi.gfx_buffer_result_ok;
}
pub fn abort(identity: Owner, input: *const abi.GfxOwnedBufferReservation, quiesced: u32) i32 {
    if (quiesced > 1) return abi.gfx_buffer_error_invalid;
    const ticket = creation(input) catch |err| return api.status(err);
    const call = context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(call);
    buffers.lock();
    defer buffers.unlock();
    buffers.store.abortOwned(ticket, identity, quiesced == 1) catch |err| return api.status(err);
    return abi.gfx_buffer_result_ok;
}
pub fn take(identity: Owner, adapter: u32, generation: u64, output: *abi.GfxOwnedBufferRelease) i32 {
    if (!api.validOutput(abi.GfxOwnedBufferRelease, output)) return abi.gfx_buffer_error_invalid;
    const call = context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(call);
    const ticket = blk: {
        buffers.lock();
        defer buffers.unlock();
        const found = buffers.store.takeOwnedRelease(identity, .{ .adapter = adapter, .driver_owner = @intCast(identity.id), .device_generation = generation }) catch |err| return api.status(err);
        break :blk found orelse return abi.gfx_buffer_error_busy;
    };
    output.* = .{ .buffer = api.publicHandle(ticket.release.buffer), .cookie = ticket.release.backing.cookie,
        .byte_length = ticket.release.backing.bytes, .attempt = ticket.release.attempt, .device_generation = generation,
        .driver_generation = identity.generation, .adapter_id = adapter, .driver_owner = @intCast(identity.id) };
    return abi.gfx_buffer_result_ok;
}
pub fn finish(identity: Owner, input: *const abi.GfxOwnedBufferRelease, quiesced: u32) i32 {
    if (@intFromPtr(input) == 0 or quiesced > 1) return abi.gfx_buffer_error_invalid;
    const v = input.*;
    if (v.version != 1 or v.size < @sizeOf(abi.GfxOwnedBufferRelease) or v.reserved0 != 0) return abi.gfx_buffer_error_invalid;
    const owner: Owner = .{ .kind = .driver, .id = v.driver_owner, .generation = v.driver_generation };
    const ticket: buffers.lifetime.OwnedRelease = .{
        .release = .{ .buffer = api.handle(v.buffer) catch |err| return api.status(err),
            .backing = .{ .cookie = v.cookie, .bytes = v.byte_length, .driver = owner }, .attempt = v.attempt },
        .driver = owner, .binding = .{ .adapter = v.adapter_id, .driver_owner = v.driver_owner, .device_generation = v.device_generation } };
    const call = context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = context.leaveUnwind(call);
    buffers.lock();
    defer buffers.unlock();
    buffers.store.finishOwnedRelease(ticket, identity, quiesced == 1) catch |err| return api.status(err);
    return abi.gfx_buffer_result_ok;
}
