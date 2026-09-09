// Resident backing and lifetime bridge. Rendering, GPU page tables and
// placement policy belong to R4GFX and the device driver, not this owner.
const std = @import("std");
const builtin = @import("builtin");
const virt = @import("virt.zig");
const phys = @import("phys.zig");
const paging = @import("paging.zig");
const sync = @import("../sched/sync.zig");
const calls = @import("../display/ownership.zig");
pub const lifetime = @import("gfx_buffer_owner.zig");
pub const layout = lifetime.layout;
pub const Owner = lifetime.Owner;
pub const Handle = lifetime.Handle;
pub const Error = lifetime.Error || error{OutOfMemory};
pub const Created = struct { buffer: Handle, reference: Handle, address: u64, bytes: u64 };

// A finite admission budget, distinct from an allocation guarantee. Failed
// TLB release remains charged. The allocator also retains system headroom.
pub const total_budget: u64 = 1024 * 1024 * 1024;
pub const producer_budget: u64 = 256 * 1024 * 1024;
pub const system_reserve: u64 = 16 * 1024 * 1024;
pub const capacity: usize = 256;
pub const Store = lifetime.Table(capacity, 1024, 2048);
pub var store = Store{ .budget_bytes = total_budget, .producer_budget_bytes = producer_budget };

// Legacy shared raster metadata uses this very same owner, so its immutable
// generation and the BO lease are published in one critical section.
pub var metadata_lock = sync.Mutex.initClass("gfx-buffer-owner", sync.LockRank.program_instances, .no_sleep);
pub const raster_owner = Owner{ .kind = .kernel, .id = 1, .generation = 1 };

pub fn lock() void {
    while (!metadata_lock.tryLock()) asm volatile ("pause");
}
pub fn unlock() void {
    _ = metadata_lock.unlock();
}

pub fn create(owner: Owner, descriptor: layout.Descriptor) Error!Created {
    if (descriptor.location != .system or !descriptor.binding.portable()) return error.Unsupported;
    const call = calls.retainCall();
    if (!call.admitted()) return error.Busy;
    defer calls.releaseCall(call);
    collect();
    lock();
    const ticket = store.begin(owner, descriptor) catch |err| {
        unlock();
        return err;
    };
    unlock();
    const free_bytes = phys.stats().free_frames * paging.PAGE_SIZE;
    if (free_bytes <= system_reserve or ticket.bytes > free_bytes - system_reserve) {
        abort(ticket);
        return error.OutOfMemory;
    }
    // This range has its own device owner. Program teardown cannot release
    // its pages while a consumer reference or device lease survives.
    const range = virt.reserve(.{
        .window = .graphics,
        .len = ticket.bytes,
        .alignment = @max(@as(u64, paging.PAGE_SIZE), descriptor.alignment),
        .kind = .dma,
        .owner = .device,
        .owner_id = ticket.buffer.generation,
        .name = "graphics-buffer",
    }) catch {
        abort(ticket);
        return error.OutOfMemory;
    };
    const address = (virt.rangeInfo(range) orelse unreachable).base;
    var committed: u64 = 0;
    while (committed < ticket.bytes) {
        const count = @min(ticket.bytes - committed, 64 * paging.PAGE_SIZE);
        virt.commit(range, committed, count) catch {
            // Even a partially failed allocation has a real owner until VM
            // release confirms every CPU's TLB acknowledgement.
            lock();
            store.publish(ticket, .{ .cookie = range, .bytes = ticket.bytes }) catch unreachable;
            store.drop(ticket.reference, owner) catch {};
            unlock();
            collect();
            return error.OutOfMemory;
        };
        committed += count;
    }
    lock();
    defer unlock();
    store.publish(ticket, .{ .cookie = range, .cpu_address = address, .bytes = ticket.bytes, .cache = .write_back }) catch unreachable;
    _ = try store.describe(ticket.reference, owner);
    return .{ .buffer = ticket.buffer, .reference = ticket.reference, .address = address, .bytes = descriptor.bytes };
}

fn abort(ticket: lifetime.Create) void {
    lock();
    defer unlock();
    store.abort(ticket) catch unreachable;
}

// CPU visibility is explicit at admission and completion. x86 coherent WB
// pages need ordering; device-private caches and GPU TLBs still require a
// driver completion before endDevice. No CPU access to unmapped VRAM.
pub fn visibility() void {
    if (builtin.cpu.arch == .x86_64) asm volatile ("mfence" ::: .{ .memory = true });
}

pub fn mapLocked(reference: Handle, owner: Owner, access: lifetime.Access, offset: u64, bytes: u64) Error!lifetime.Use {
    const result = try store.use(reference, owner, access, offset, bytes);
    visibility();
    return result;
}

pub fn unmapCpuLocked(lease: Handle, owner: Owner) Error!void {
    const use = try store.useInfo(lease, owner);
    if (use.access != .cpu_read and use.access != .cpu_write) return error.Invalid;
    visibility();
    try store.endUse(lease, owner, false);
}

pub fn drop(reference: Handle, owner: Owner) Error!void {
    const call = calls.retainCall();
    if (!call.admitted()) return error.Busy;
    defer calls.releaseCall(call);
    lock();
    store.drop(reference, owner) catch |err| {
        unlock();
        return err;
    };
    unlock();
    collect();
}

pub fn stopped(owner: Owner) void {
    lock();
    visibility();
    store.stoppedOwner(owner);
    unlock();
    collect();
}

pub fn collect() void {
    const call = calls.retainCall();
    if (!call.admitted()) return;
    defer calls.releaseCall(call);
    // A failed release is retried by the next resource operation. A claimed
    // ticket excludes concurrent collectors; never spin on a TLB timeout.
    var count: usize = 0;
    while (count < capacity) : (count += 1) {
        lock();
        const ticket = store.pendingRelease() orelse {
            unlock();
            return;
        };
        unlock();
        // Device-local destruction is supplied by its retained driver in
        // the later backend registration; it must not look like a VM ID.
        const released = if (ticket.backing.driver != null) false else blk: {
            virt.release(@intCast(ticket.backing.cookie)) catch break :blk false;
            break :blk true;
        };
        lock();
        store.finishRelease(ticket, released) catch {};
        unlock();
        if (!released) return;
    }
}

pub fn retainsDriver(owner: u32) bool {
    lock();
    defer unlock();
    // The loader's owner ID is reusable; the driver memory bridge supplies
    // a separate nonwrapping epoch. Any surviving driver use vetoes reuse.
    for (store.objects) |object| if (object.backing) |backing| if (backing.driver) |driver| {
        if (driver.id == owner) return true;
    };
    for (store.leases) |lease| {
        if (lease.handle.id != 0 and lease.owner.kind == .driver and lease.owner.id == owner) return true;
    }
    for (store.references) |reference| {
        if (reference.handle.id != 0 and reference.owner.kind == .driver and reference.owner.id == owner) return true;
    }
    return false;
}
