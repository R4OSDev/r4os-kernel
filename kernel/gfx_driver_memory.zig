// Owner-authenticated memory operations behind DriverApi's optional v25
// query. The enclosing DriverApi execution guard serializes this state.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const api = @import("../program/gfx_buffer_api.zig");
const paging = @import("../memory/paging.zig");
const boot = @import("../bootloader/boot_info.zig");
const cpu = @import("../platform/cpu.zig");
const windows = @import("mmio_windows.zig");
pub const Owner = buffers.Owner;
const Epoch = struct { id: u32 = 0, generation: u64 = 0, closing: bool = false };
var epochs: [128]Epoch = .{Epoch{}} ** 128;
var serial: u64 = 0;
const Device = struct { owner: Owner = .{ .kind = .driver, .id = 0, .generation = 0 }, descriptor: abi.GfxDeviceLease = .{} };
var devices: [1024]Device = .{Device{}} ** 1024;

pub fn owner(id: u32, admission: bool) buffers.Error!Owner {
    if (id == 0) return error.Invalid;
    for (epochs) |epoch| if (epoch.id == id) {
        if (admission and epoch.closing) return error.Closed;
        return .{ .kind = .driver, .id = id, .generation = epoch.generation };
    };
    if (!admission) return error.Stale;
    if (serial == std.math.maxInt(u64)) return error.Exhausted;
    for (&epochs) |*epoch| if (epoch.id == 0) {
        serial += 1;
        epoch.* = .{ .id = id, .generation = serial };
        return .{ .kind = .driver, .id = id, .generation = serial };
    };
    return error.Capacity;
}
pub fn beginClose(id: u32) void {
    for (&epochs) |*epoch| if (epoch.id == id) {
        epoch.closing = true;
    };
}
pub fn retained(id: u32) bool {
    const identity = owner(id, false) catch return false;
    for (devices) |item| if (item.descriptor.lease.id != 0 and item.owner.eql(identity)) return true;
    return mmio.retains(identity) or buffers.retainsDriver(id);
}
pub fn finishOwner(id: u32) void {
    const identity = owner(id, false) catch return;
    buffers.stopped(identity);
    for (&epochs) |*epoch| if (epoch.id == id) {
        epoch.* = .{};
    };
}

pub fn acquire(identity: Owner, reference: *const abi.GfxBufferHandle, request_ptr: *const abi.GfxDeviceRequest, output: *abi.GfxDeviceLease) i32 {
    if (@intFromPtr(reference) == 0 or @intFromPtr(request_ptr) == 0 or !api.validOutput(abi.GfxDeviceLease, output)) return abi.gfx_buffer_error_invalid;
    const request = request_ptr.*;
    const ref = api.handle(reference.*) catch |err| return api.status(err);
    if (request.version != 1 or request.size < @sizeOf(abi.GfxDeviceRequest) or request.reserved0 != 0 or
        request.adapter_id == 0 or request.device_generation == 0 or request.access > 3 or request.address_space > 1 or
        (request.address_space == 0 and (request.gpu_virtual_address != 0 or request.access == 3)) or
        (request.address_space == 1 and (request.gpu_virtual_address == 0 or request.gpu_virtual_address > std.math.maxInt(u64) - request.byte_length)) or
        (request.access == 3 and (request.byte_offset | request.byte_length | request.gpu_virtual_address) % paging.PAGE_SIZE != 0)) return abi.gfx_buffer_error_invalid;
    var free: ?*Device = null;
    for (&devices) |*item| if (item.descriptor.lease.id == 0) {
        free = item;
        break;
    };
    const record = free orelse return abi.gfx_buffer_error_capacity;
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
        3 => .device_mapping,
        else => unreachable,
    };
    const use = buffers.mapLocked(ref, identity, access, request.byte_offset, request.byte_length) catch |err| {
        buffers.unlock();
        return api.status(err);
    };
    buffers.unlock();
    // Admission retains the backing during this bounded page walk. No CPU
    // pointer is ever published as a GPU VA or substituted for a DMA page.
    if (request.address_space == 0) {
        var offset: u64 = 0;
        while (offset < request.byte_length) {
            const piece = dmaSegment(use, offset, request.dma_mask) catch |err| {
                buffers.lock();
                buffers.store.endUse(use.lease, identity, true) catch unreachable;
                buffers.unlock();
                return api.status(err);
            };
            offset = piece.next_offset;
        }
    }
    record.* = .{ .owner = identity, .descriptor = .{
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
    } };
    output.* = record.descriptor;
    return abi.gfx_buffer_result_ok;
}
fn matching(identity: Owner, input: *const abi.GfxDeviceLease) ?*Device {
    if (@intFromPtr(input) == 0 or input.version != 1 or input.size < @sizeOf(abi.GfxDeviceLease)) return null;
    var normalized = input.*;
    normalized.size = @sizeOf(abi.GfxDeviceLease);
    for (&devices) |*item| if (item.descriptor.lease.id != 0 and item.owner.eql(identity) and std.meta.eql(item.descriptor, normalized)) return item;
    return null;
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
    const record = matching(identity, input) orelse return abi.gfx_buffer_error_stale;
    if (record.descriptor.address_space != 0) return abi.gfx_buffer_error_unsupported;
    const value = blk: {
        buffers.lock();
        defer buffers.unlock();
        const use = buffers.store.useInfo(api.handle(record.descriptor.lease) catch unreachable, identity) catch |err| return api.status(err);
        break :blk dmaSegment(use, offset, record.descriptor.dma_mask) catch |err| return api.status(err);
    };
    output.* = value;
    return abi.gfx_buffer_result_ok;
}
pub fn release(identity: Owner, input: *const abi.GfxDeviceLease, quiesced: u32) i32 {
    const record = matching(identity, input) orelse return abi.gfx_buffer_error_stale;
    if (quiesced != 1) return if (quiesced == 0) abi.gfx_buffer_error_busy else abi.gfx_buffer_error_invalid;
    buffers.lock();
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
                if (!cpu.patAvailable() or ((cpu.status().pat_msr >> 16) & 0xFF) != 0) return false;
                break :blk paging.CACHE_DISABLE;
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
    const result = mmio.create(&backend, identity, .{ .resource_base = input.resource_base, .resource_bytes = input.resource_bytes, .offset = input.byte_offset, .bytes = input.byte_length, .policy = cache, .prefetchable = (input.resource_flags & 1) != 0 }) catch |err| return windowStatus(err);
    output.* = .{ .handle = api.publicHandle(result.handle), .cpu_address = result.cpu, .physical_address = result.physical, .byte_length = result.bytes, .cache_policy = input.cache_policy, .flags = if (result.borrowed) 1 else 0 };
    return abi.gfx_buffer_result_ok;
}
pub fn unmapWindow(identity: Owner, input: *const abi.GfxBufferHandle, quiesced: u32) i32 {
    if (@intFromPtr(input) == 0 or quiesced > 1) return abi.gfx_buffer_error_invalid;
    const handle = api.handle(input.*) catch |err| return api.status(err);
    mmio.release(&backend, identity, handle, quiesced == 1) catch |err| return windowStatus(err);
    return abi.gfx_buffer_result_ok;
}
pub fn collect(identity: Owner) i32 {
    buffers.collect();
    return if (mmio.collect(&backend, identity)) abi.gfx_buffer_result_ok else abi.gfx_buffer_error_busy;
}
