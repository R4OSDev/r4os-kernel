// Resident backing and lifetime bridge. Rendering, GPU page tables and
// placement policy belong to R4GFX and the device driver, not this owner.
const std = @import("std");
const builtin = @import("builtin");
const virt = @import("virt.zig");
const phys = @import("phys.zig");
const paging = @import("paging.zig");
const sync = @import("../sched/sync.zig");
const calls = @import("../display/ownership.zig");
const heap = @import("heap.zig");
pub const lifetime = @import("gfx_buffer_owner.zig");
pub const layout = lifetime.layout;
pub const Owner = lifetime.Owner;
pub const Handle = lifetime.Handle;
pub const Error = lifetime.Error || error{OutOfMemory};
pub const Created = struct { buffer: Handle, reference: Handle, address: u64, bytes: u64 };

// A finite admission budget, distinct from an allocation guarantee. Failed
// TLB release remains charged. The allocator also retains system headroom.
// A producer can retain a 1 GB buffer alongside its other graphics resources.
// These ceilings reserve no RAM; every creation still checks physical headroom.
pub const total_budget: u64 = 4 * 1024 * 1024 * 1024;
pub const producer_budget: u64 = 2 * 1024 * 1024 * 1024;
pub const system_reserve: u64 = 16 * 1024 * 1024;
pub const Store = lifetime.DynamicTable();
pub var store = Store{ .budget_bytes = total_budget, .producer_budget_bytes = producer_budget };
pub const Need = struct { objects: usize = 0, references: usize = 0, leases: usize = 0 };
const Growth = struct { busy: bool = false, owned: ?[]u8 = null, retired: ?[]u8 = null };
var growth: [3]Growth = @splat(.{});

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

/// Prepare resident metadata before entering any graphics transaction. Heap
/// allocation/free never spans the BO owner, and the caller is kill-protected
/// until publication/release bookkeeping has finished. Growth keeps handles,
/// generations and backing owners; only internal, lock-scoped POD records move.
fn prepare(need: Need) Error!void {
    if (@import("../kernel/irq_router.zig").inDispatch()) return error.Busy;
    const call = calls.retainCall();
    if (!call.admitted()) return error.Busy;
    defer calls.releaseCall(call);
    inline for (std.enums.values(lifetime.Pool)) |kind| {
        const needed = @field(need, @tagName(kind));
        if (needed != 0) try preparePool(kind, needed);
    }
}
/// On success the caller owns metadata_lock and all requested free slots.
/// Recheck after allocation: another CPU may have consumed slots while the
/// heap operation ran. Contention is bounded and never reported as heap OOM.
pub fn lockPrepared(need: Need) Error!void {
    if (@import("../kernel/irq_router.zig").inDispatch()) return error.Busy;
    for (0..4) |_| {
        lock();
        const ready = inline for (std.enums.values(lifetime.Pool)) |kind| {
            const needed = @field(need, @tagName(kind));
            if (needed != 0 and store.freeSlots(kind, needed) < needed) break false;
        } else true;
        if (ready) return;
        unlock();
        try prepare(need);
    }
    return error.Busy;
}
fn preparePool(comptime kind: lifetime.Pool, needed: usize) Error!void {
    const T = Store.Element(kind);
    const control = &growth[@intFromEnum(kind)];
    lock();
    if (control.busy) { unlock(); return error.Busy; }
    if (store.freeSlots(kind, needed) >= needed) { unlock(); return; }
    const old_count = @field(store, @tagName(kind)).len;
    const added = std.math.add(usize, old_count, needed) catch { unlock(); return error.Exhausted; };
    const count = @max(64, @max(added, old_count *| 2));
    if (count > std.math.maxInt(u32)) { unlock(); return error.Exhausted; }
    const bytes = std.math.mul(usize, count, @sizeOf(T)) catch { unlock(); return error.Exhausted; };
    control.busy = true;
    const pending = control.retired;
    unlock();
    defer { lock(); control.busy = false; unlock(); }
    if (pending) |old| {
        if (heap.free(old) != .ok) return error.Busy;
        lock(); control.retired = null; unlock();
    }
    const allocation = heap.alloc(bytes, @alignOf(T)) orelse return error.OutOfMemory;
    const values: [*]T = @ptrCast(@alignCast(allocation.ptr));
    // Initializing the new tail has no shared state and needs no BO owner.
    @memset(values[old_count..count], .{});
    lock();
    _ = store.replaceStorage(kind, values[0..count]) catch |err| {
        // These extents come from distinct live heap allocations. Retain any
        // unexpected failed publication until its exact heap release succeeds.
        control.retired = allocation;
        unlock();
        return err;
    };
    const detached = control.owned;
    control.owned = allocation;
    control.retired = detached;
    unlock();
    if (detached) |old| {
        if (heap.free(old) != .ok) return error.Busy;
        lock(); control.retired = null; unlock();
    }
}

pub fn create(owner: Owner, descriptor: layout.Descriptor) Error!Created {
    if (descriptor.location != .system or !descriptor.binding.portable()) return error.Unsupported;
    const call = calls.retainCall();
    if (!call.admitted()) return error.Busy;
    defer calls.releaseCall(call);
    collect();
    try lockPrepared(.{ .objects = 1, .references = 1 });
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
    if (!lifetime.isCpu(use.access)) return error.Invalid;
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
    defer collectMetadata();
    // A failed release is retried by the next resource operation. A claimed
    // ticket excludes concurrent collectors; never spin on a TLB timeout.
    lock();
    const budget = store.objects.len;
    unlock();
    var count: usize = 0;
    while (count < budget) : (count += 1) {
        lock();
        const ticket = store.pendingSystemRelease() orelse {
            unlock();
            return;
        };
        unlock();
        const released = blk: {
            virt.release(@intCast(ticket.backing.cookie)) catch break :blk false;
            break :blk true;
        };
        lock();
        store.finishRelease(ticket, released) catch {};
        unlock();
        if (!released) return;
    }
}

// Empty metadata must not become permanent heap consumption after the last
// process/driver has retired. Serial generations survive this release. A
// failed heap free retains its exact descriptor for a subsequent collection.
fn emptyLocked() bool {
    if (store.committed_bytes != 0) return false;
    inline for (std.enums.values(lifetime.Pool)) |kind| {
        const len = @field(store, @tagName(kind)).len;
        if (store.freeSlots(kind, len) != len) return false;
    }
    return true;
}
fn collectMetadata() void {
    inline for (std.enums.values(lifetime.Pool)) |kind| {
        const control = &growth[@intFromEnum(kind)];
        lock();
        if (control.busy) {
            unlock();
        } else {
            if (control.retired == null and emptyLocked()) {
                control.retired = control.owned;
                control.owned = null;
                @field(store, @tagName(kind)) = &.{};
            }
            if (control.retired) |allocation| {
                control.busy = true;
                unlock();
                const released = heap.free(allocation) == .ok;
                lock();
                if (released) control.retired = null;
                control.busy = false;
                unlock();
            } else unlock();
        }
    }
}

pub fn retainsDriver(owner: u32) bool {
    lock();
    defer unlock();
    // The loader's owner ID is reusable; the driver memory bridge supplies
    // the actual nonwrapping driver-start epoch. Any surviving driver use vetoes reuse.
    for (store.objects) |object| {
        if (object.phase != .empty and object.owned_cookie != 0 and
            object.producer.kind == .driver and object.producer.id == owner) return true;
        if ((object.phase == .releasing or object.phase == .destroying) and
            object.producer.kind == .driver and object.producer.id == owner) return true;
        if (object.backing) |backing| if (backing.driver) |driver| {
            if (driver.id == owner) return true;
        };
    }
    for (store.leases) |lease| {
        if (lease.handle.id != 0 and lease.owner.kind == .driver and lease.owner.id == owner) return true;
    }
    for (store.references) |reference| {
        if (reference.handle.id != 0 and reference.owner.kind == .driver and reference.owner.id == owner) return true;
    }
    return false;
}

pub fn pendingReleaseForOwner(owner: Owner) bool {
    lock();
    defer unlock();
    return store.pendingReleaseForOwner(owner);
}
