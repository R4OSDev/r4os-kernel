// Owner-authenticated memory operations in Init/Work/Shutdown. The shared
// BO lock protects driver/device metadata; no page walk or external resource
// operation spans it. MMIO alone retains its legacy execution guard.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const api = @import("../program/gfx_buffer_api.zig");
const paging = @import("../memory/paging.zig");
const boot = @import("../bootloader/boot_info.zig");
const cpu = @import("../platform/cpu.zig");
const windows = @import("mmio_windows.zig");
const ownership = @import("gfx_driver_memory_owner.zig");
const task_context = @import("../sched/task_context.zig");
const scheduler = @import("../sched/scheduler.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
pub const Owner = buffers.Owner;
var state: ownership.State(@import("../driver/registry.zig").MAX_DRIVERS, 1024) = .{};

const LifecycleToken = union(enum) { boot: u64, task };
fn lifecycleLock() LifecycleToken {
    if (scheduler.current() == null) {
        // Preloaded XHCI/storage drivers bind before Task/SMP admission.
        // No BO operation is admitted here, and no worker can coexist.
        const flags = interrupts.saveAndDisableLocal();
        std.debug.assert(!interrupts.runtimeSerializationEnabled());
        return .{ .boot = flags };
    }
    buffers.lock();
    return .task;
}
fn lifecycleUnlock(token: LifecycleToken) void {
    switch (token) {
        .boot => |flags| interrupts.restoreLocal(flags),
        .task => buffers.unlock(),
    }
}
pub fn bind(id: u32, generation: u64) bool {
    const token = lifecycleLock();
    defer lifecycleUnlock(token);
    return state.bind(id, generation);
}
// Only the loader's unpublished bind rollback may use this entrypoint.
pub fn discardEmptyBinding(id: u32, generation: u64) bool {
    const token = lifecycleLock();
    defer lifecycleUnlock(token);
    return state.retire(.{ .kind = .driver, .id = id, .generation = generation });
}
pub fn owner(id: u32, admission: bool) buffers.Error!Owner {
    const token = lifecycleLock();
    defer lifecycleUnlock(token);
    return state.owner(id, admission);
}
pub fn beginClose(id: u32) void {
    const token = lifecycleLock();
    defer lifecycleUnlock(token);
    state.close(id);
}
pub fn retained(id: u32) bool {
    const held = blk: {
        const token = lifecycleLock();
        defer lifecycleUnlock(token);
        const identity = state.owner(id, false) catch return false;
        break :blk state.retains(identity);
    };
    return held or (scheduler.current() != null and buffers.retainsDriver(id));
}
pub fn finishOwner(id: u32) void {
    const identity = owner(id, false) catch return;
    if (scheduler.current() != null) buffers.stopped(identity);
    const token = lifecycleLock();
    defer lifecycleUnlock(token);
    std.debug.assert(state.retire(identity));
}

pub fn acquire(identity: Owner, reference: *const abi.GfxBufferHandle, request_ptr: *const abi.GfxDeviceRequest, output: *abi.GfxDeviceLease) i32 {
    if (@intFromPtr(reference) == 0 or @intFromPtr(request_ptr) == 0 or !api.validOutput(abi.GfxDeviceLease, output)) return abi.gfx_buffer_error_invalid;
    const request = request_ptr.*;
    const ref = api.handle(reference.*) catch |err| return api.status(err);
    if (request.version != 1 or request.size < @sizeOf(abi.GfxDeviceRequest) or request.reserved0 != 0 or
        request.adapter_id == 0 or request.device_generation == 0 or request.access > 4 or request.address_space > 1 or
        (request.address_space == 0 and (request.gpu_virtual_address != 0 or request.access == 3)) or
        (request.address_space == 1 and (request.gpu_virtual_address == 0 or request.gpu_virtual_address > std.math.maxInt(u64) - request.byte_length)) or
        (request.access == 4 and request.address_space != 0) or
        (request.access == 3 and (request.byte_offset | request.byte_length | request.gpu_virtual_address) % paging.PAGE_SIZE != 0)) return abi.gfx_buffer_error_invalid;
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = task_context.leaveUnwind(call);
    buffers.lock();
    const desc = buffers.store.describe(ref, identity) catch |err| {
        buffers.unlock();
        return api.status(err);
    };
    if (!desc.binding.portable() and (desc.binding.adapter != request.adapter_id or desc.binding.driver_owner != identity.id or desc.binding.device_generation != request.device_generation)) {
        buffers.unlock();
        return abi.gfx_buffer_error_stale;
    }
    const access: buffers.lifetime.Access = switch (request.access) {
        0 => .device_read,
        1 => .device_write,
        2 => .scanout,
        3, 4 => .device_mapping,
        else => unreachable,
    };
    const use = buffers.mapLocked(ref, identity, access, request.byte_offset, request.byte_length) catch |err| {
        buffers.unlock();
        return api.status(err);
    };
    const descriptor: abi.GfxDeviceLease = .{
        .lease = api.publicHandle(use.lease),
        .byte_offset = request.byte_offset,
        .byte_length = request.byte_length,
        .gpu_virtual_address = request.gpu_virtual_address,
        .device_generation = request.device_generation,
        .adapter_id = request.adapter_id,
        .driver_owner = @intCast(identity.id),
        .access = request.access,
        .address_space = request.address_space,
        .dma_mask = request.dma_mask,
    };
    const record = state.reserve(identity, descriptor) catch |err| {
        buffers.store.endUse(use.lease, identity, true) catch unreachable;
        buffers.unlock();
        return api.status(err);
    };
    buffers.unlock();
    // Both the backing and a busy device slot survive this page walk.
    // Another admission cannot reuse the slot, even if this one fails.
    if (request.address_space == 0) {
        var offset: u64 = 0;
        while (offset < request.byte_length) {
            const piece = dmaSegment(use, offset, request.dma_mask) catch |err| {
                buffers.lock();
                buffers.store.endUse(use.lease, identity, true) catch unreachable;
                record.* = .{};
                buffers.unlock();
                buffers.collect();
                return api.status(err);
            };
            offset = piece.next_offset;
        }
    }
    buffers.lock();
    record.busy = false;
    buffers.unlock();
    output.* = descriptor;
    return abi.gfx_buffer_result_ok;
}
fn leaseValue(input: *const abi.GfxDeviceLease) ?abi.GfxDeviceLease {
    if (@intFromPtr(input) == 0 or input.version != 1 or input.size < @sizeOf(abi.GfxDeviceLease)) return null;
    var normalized = input.*;
    normalized.size = @sizeOf(abi.GfxDeviceLease);
    return normalized;
}
pub fn dmaSegment(use: buffers.lifetime.Use, offset: u64, mask: u64) buffers.Error!abi.GfxDmaSegment {
    if (offset >= use.range.bytes or use.backing.cpu_address == 0 or use.backing.cache != .write_back or use.backing.driver != null) return error.Unsupported;
    const address = use.backing.cpu_address + use.range.offset + offset;
    const inside = address % paging.PAGE_SIZE;
    const physical = paging.mappedFrame(address - inside) orelse return error.Stale;
    const bytes = @min(paging.PAGE_SIZE - inside, use.range.bytes - offset);
    const dma_address = std.math.add(u64, physical, inside) catch return error.Overflow;
    if (dma_address > mask or bytes - 1 > mask - dma_address) return error.Unsupported;
    return .{ .dma_address = dma_address, .byte_length = bytes, .next_offset = offset + bytes };
}
pub fn segment(identity: Owner, input: *const abi.GfxDeviceLease, offset: u64, output: *abi.GfxDmaSegment) i32 {
    if (!api.validOutput(abi.GfxDmaSegment, output)) return abi.gfx_buffer_error_invalid;
    const descriptor = leaseValue(input) orelse return abi.gfx_buffer_error_stale;
    const value = blk: {
        buffers.lock();
        defer buffers.unlock();
        const record = state.matching(identity, descriptor) catch |err| return api.status(err);
        if (record.descriptor.address_space != 0) return abi.gfx_buffer_error_unsupported;
        const use = buffers.store.useInfo(api.handle(record.descriptor.lease) catch unreachable, identity) catch |err| return api.status(err);
        break :blk dmaSegment(use, offset, record.descriptor.dma_mask) catch |err| return api.status(err);
    };
    output.* = value;
    return abi.gfx_buffer_result_ok;
}
pub fn release(identity: Owner, input: *const abi.GfxDeviceLease, quiesced: u32) i32 {
    const descriptor = leaseValue(input) orelse return abi.gfx_buffer_error_stale;
    const call = task_context.enterUnwind();
    if (!call.admitted()) return abi.gfx_buffer_error_busy;
    defer _ = task_context.leaveUnwind(call);
    buffers.lock();
    const record = state.matching(identity, descriptor) catch |err| {
        buffers.unlock();
        return api.status(err);
    };
    if (quiesced != 1) {
        buffers.unlock();
        return if (quiesced == 0) abi.gfx_buffer_error_busy else abi.gfx_buffer_error_invalid;
    }
    buffers.visibility();
    buffers.store.endUse(api.handle(record.descriptor.lease) catch unreachable, identity, true) catch |err| {
        buffers.unlock();
        return api.status(err);
    };
    record.* = .{};
    buffers.unlock();
    buffers.collect();
    return abi.gfx_buffer_result_ok;
}

const Backend = struct {
    pub fn deviceSpan(_: *@This(), base: u64, bytes: u64) bool {
        for (boot.memoryMap()) |entry| {
            if (!entry.valid or base >= entry.end or entry.base >= base + bytes) continue;
            switch (entry.kind) {
                .reserved, .framebuffer => {},
                else => return false,
            }
        }
        return true;
    }
    pub fn cpuAddress(_: *@This(), physical: u64, bytes: u64) ?u64 {
        const address = boot.physToHhdm(physical) orelse return null;
        _ = std.math.add(u64, address, bytes) catch return null;
        return address;
    }
    pub fn policy(_: *@This(), address: u64) ?windows.Policy {
        const selector = paging.cacheSelector(address) orelse return null;
        const physical = paging.physicalAddress(address) orelse return .other;
        if (boot.hhdmToPhys(address) != physical) return .other;
        const pat = cpu.status().pat_msr;
        const memory_type: u8 = @truncate(pat >> (@as(u6, selector) * 8));
        return switch (memory_type) {
            0 => .uncached,
            1 => .write_combining,
            else => .other,
        };
    }
    pub fn map(_: *@This(), address: u64, physical: u64, cache: windows.Policy) bool {
        const flags: u64 = switch (cache) {
            .uncached => blk: {
                if (!cpu.patAvailable()) return false;
                const selector = windows.cacheSelector(cpu.status().pat_msr, 0) orelse return false;
                break :blk (if (selector & 1 != 0) paging.WRITE_THROUGH else @as(u64, 0)) |
                    (if (selector & 2 != 0) paging.CACHE_DISABLE else @as(u64, 0)) |
                    (if (selector & 4 != 0) paging.PAGE_ATTRIBUTE_TABLE else @as(u64, 0));
            },
            .write_combining => blk: {
                if (!cpu.writeCombiningBasisAvailable()) return false;
                break :blk paging.WRITE_THROUGH | paging.PAGE_ATTRIBUTE_TABLE;
            },
            .other => return false,
        };
        return paging.mapPage(address, physical, paging.WRITABLE | paging.NO_EXECUTE | flags);
    }
    pub fn unmap(_: *@This(), address: u64) bool {
        return paging.unmapPage(address);
    }
};
var backend = Backend{};
var mmio = windows.Manager(Backend){};
fn windowStatus(err: windows.Error) i32 {
    return switch (err) {
        error.Invalid => abi.gfx_buffer_error_invalid,
        error.Overflow, error.Exhausted => abi.gfx_buffer_error_overflow,
        error.Unsupported => abi.gfx_buffer_error_unsupported,
        error.Busy => abi.gfx_buffer_error_busy,
        error.Capacity => abi.gfx_buffer_error_capacity,
        error.Stale => abi.gfx_buffer_error_stale,
        error.MapFailed => abi.gfx_buffer_error_oom,
    };
}
pub fn mapWindow(identity: Owner, input: *const abi.GfxMmioRequest, output: *abi.GfxMmioWindow) i32 {
    if (@intFromPtr(input) == 0 or input.version != 1 or input.size < @sizeOf(abi.GfxMmioRequest) or
        (input.resource_flags & ~@as(u32, 1)) != 0 or !api.validOutput(abi.GfxMmioWindow, output)) return abi.gfx_buffer_error_invalid;
    const cache: windows.Policy = switch (input.cache_policy) {
        abi.gfx_buffer_cache_uncached => .uncached,
        abi.gfx_buffer_cache_write_combining => .write_combining,
        else => return abi.gfx_buffer_error_unsupported,
    };
    beginMmio(identity);
    defer endMmio(identity);
    const result = mmio.create(&backend, identity, .{ .resource_base = input.resource_base, .resource_bytes = input.resource_bytes, .offset = input.byte_offset, .bytes = input.byte_length, .policy = cache, .prefetchable = (input.resource_flags & 1) != 0 }) catch |err| return windowStatus(err);
    output.* = .{ .handle = api.publicHandle(result.handle), .cpu_address = result.cpu, .physical_address = result.physical, .byte_length = result.bytes, .cache_policy = input.cache_policy, .flags = if (result.borrowed) 1 else 0 };
    return abi.gfx_buffer_result_ok;
}
pub fn unmapWindow(identity: Owner, input: *const abi.GfxBufferHandle, quiesced: u32) i32 {
    if (@intFromPtr(input) == 0 or quiesced > 1) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    beginMmio(identity);
    defer endMmio(identity);
    mmio.release(&backend, identity, handle, quiesced == 1) catch |err| return windowStatus(err);
    return abi.gfx_buffer_result_ok;
}
pub fn collect(identity: Owner) i32 {
    // Only Init/Shutdown may invoke the MMIO manager. Publish its status
    // for concurrent Work collectors without extending MMIO admission.
    beginMmio(identity);
    buffers.collect();
    const windows_released = mmio.collect(&backend, identity);
    endMmio(identity);
    return if (windows_released and !buffers.pendingReleaseForOwner(identity)) abi.gfx_buffer_result_ok else abi.gfx_buffer_error_busy;
}
pub fn collectBuffers(identity: Owner) i32 {
    buffers.collect();
    const pending = blk: {
        buffers.lock();
        defer buffers.unlock();
        break :blk state.pendingMmio(identity);
    };
    return if (!pending and !buffers.pendingReleaseForOwner(identity)) abi.gfx_buffer_result_ok else abi.gfx_buffer_error_busy;
}
fn beginMmio(identity: Owner) void {
    buffers.lock();
    defer buffers.unlock();
    state.beginMmio(identity);
}
fn endMmio(identity: Owner) void {
    // The enclosing lifecycle guard owns these manager reads, too.
    const held = mmio.retains(identity);
    const pending = mmio.pending(identity);
    buffers.lock();
    defer buffers.unlock();
    state.endMmio(identity, held, pending);
}
