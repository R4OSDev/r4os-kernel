// Buffer lifetime and admission, independent of allocation or GPU registers.
// The enclosing owner serializes calls. Allocation, cache synchronization,
// callbacks and actual release happen outside its no-sleep metadata lock.
const std = @import("std");
pub const layout = @import("gfx_buffer_layout.zig");

pub const Owner = struct {
    kind: enum { kernel, program, driver },
    id: u64,
    generation: u64,
    pub fn valid(self: Owner) bool {
        return self.id != 0 and self.generation != 0;
    }
    pub fn eql(a: Owner, b: Owner) bool {
        return a.kind == b.kind and a.id == b.id and a.generation == b.generation;
    }
};
pub const Handle = struct {
    id: u32 = 0,
    generation: u64 = 0,
    pub fn eql(a: Handle, b: Handle) bool {
        return a.id == b.id and a.generation == b.generation;
    }
};
pub const Backing = struct {
    // Opaque allocator identity; CPU address zero means not CPU mapped.
    cookie: u64,
    cpu_address: u64 = 0,
    bytes: u64,
    cache: enum { unavailable, write_back, write_combining, uncached } = .unavailable,
    driver: ?Owner = null,
};
pub const Access = enum { cpu_read, cpu_write, device_read, device_write, scanout, device_mapping, queue_read, queue_write, cpu_persistent_read, cpu_persistent_write };
pub const Error = layout.Error || error{ Exhausted, Budget, Capacity, Stale, Busy, Closed, WrongOwner };
pub const Create = struct { buffer: Handle, reference: Handle, bytes: u64 };
pub const Use = struct { lease: Handle, buffer: Handle, access: Access, backing: Backing, range: layout.Range };
pub const Release = struct { buffer: Handle, backing: Backing, attempt: u64 };
pub const OwnedCreate = struct { create: Create, cookie: u64, driver: Owner, binding: layout.Binding };
pub const OwnedRelease = struct { release: Release, driver: Owner, binding: layout.Binding };
pub const BudgetSnapshot = struct { limit: u64, charged: u64, closing: bool };
pub const Stats = struct {
    objects: usize = 0, references: usize = 0, leases: usize = 0,
    bytes: u64 = 0, retained_bytes: u64 = 0,
    system_bytes: u64 = 0, device_bytes: u64 = 0,
    system_backed_bytes: u64 = 0, device_backed_bytes: u64 = 0,
    system_pinned_bytes: u64 = 0, device_pinned_bytes: u64 = 0,
    scanout_pinned_bytes: u64 = 0, device_mapped_bytes: u64 = 0,
    allocating_bytes: u64 = 0, destroying_bytes: u64 = 0,
};

test "CPU backing release failure remains visible to the exact driver epoch without public references" {
    try dynamicStorageLifetime();
    try accountingLifetime();
    try deviceBudgetLifetime();
    try ownedBackingLifetime(false);
    try ownedBackingLifetime(true);
    try resetBackingLifetime();
    const t = std.testing;
    var store = Table(2, 2, 2){ .budget_bytes = 8192, .producer_budget_bytes = 8192 };
    const owner: Owner = .{ .kind = .driver, .id = 8, .generation = 7 };
    const next: Owner = .{ .kind = .driver, .id = 8, .generation = 8 };
    const ticket = try store.begin(owner, .{ .bytes = 4096 });
    // An incomplete CPU allocation still publishes its retained VM identity.
    try store.publish(ticket, .{ .cookie = 31, .bytes = 4096 });
    try t.expect(!store.pendingReleaseForOwner(owner));
    try store.drop(ticket.reference, owner);
    try t.expectEqual(@as(usize, 0), store.stats().references);
    try t.expect(store.pendingReleaseForOwner(owner));
    try t.expect(!store.pendingReleaseForOwner(next));
    const release = store.pendingRelease().?;
    try t.expect(store.pendingReleaseForOwner(owner));
    try t.expectError(error.Busy, store.finishRelease(release, false));
    try t.expect(store.pendingReleaseForOwner(owner));
    try t.expectEqual(@as(u64, 4096), store.stats().retained_bytes);
    try store.finishRelease(store.pendingRelease().?, true);
    try t.expect(!store.pendingReleaseForOwner(owner));
    try t.expectEqual(@as(u64, 0), store.stats().bytes);
}

fn dynamicStorageLifetime() !void {
    const t = std.testing;
    const S = DynamicTable();
    var store: S = .{ .budget_bytes = 8 * 1024 * 1024, .producer_budget_bytes = 8 * 1024 * 1024 };
    defer inline for (std.enums.values(Pool)) |kind| t.allocator.free(@field(store, @tagName(kind)));
    const app: Owner = .{ .kind = .program, .id = 4, .generation = 10 };
    const driver: Owner = .{ .kind = .driver, .id = 8, .generation = 17 };
    var first: Create = undefined;
    var first_mapping: Use = undefined;
    var descriptor: layout.Descriptor = .{ .bytes = 4096, .usage = 63, .format = .argb8888,
        .width = 32, .height = 32, .plane_count = 1 };
    descriptor.planes[0].pitch = 128;
    // These counts cross the old production pools; none is an implementation
    // limit. Storage replacement must preserve IDs, generations and leases.
    for (0..513) |i| {
        inline for (std.enums.values(Pool)) |kind| {
            const needed: usize = switch (kind) { .objects => 1, .references => 4, .leases => 6 };
            if (store.freeSlots(kind, needed) < needed) {
                const size = @max(8, @field(store, @tagName(kind)).len * 2);
                const replacement = try t.allocator.alloc(S.Element(kind), size);
                @memset(replacement[@field(store, @tagName(kind)).len..], .{});
                const old = try store.replaceStorage(kind, replacement);
                t.allocator.free(old);
            }
        }
        const created = try store.begin(app, descriptor);
        try store.publish(created, .{ .cookie = i + 1, .bytes = 4096 });
        const imported = try store.share(created.reference, driver);
        _ = try store.share(imported, driver);
        _ = try store.share(imported, driver);
        for (0..5) |lease| {
            const mapping = try store.use(imported, driver, .device_mapping, 0, 4096);
            if (i == 0 and lease == 0) first_mapping = mapping;
        }
        if (i == 0) {
            first = created;
            _ = try store.use(imported, driver, .scanout, 0, 4096);
        }
        try t.expect((try store.describe(first.reference, app)).bytes == 4096);
        try t.expectEqualDeep(first_mapping, try store.useInfo(first_mapping.lease, driver));
    }
    const before = store.stats();
    try t.expect(before.objects == 513 and before.references == 2052 and before.leases == 2566 and
        before.device_mapped_bytes == 513 * 4096 and before.scanout_pinned_bytes == 4096);
    store.stoppedOwner(app);
    store.stoppedOwner(driver);
    try t.expect(store.stats().references == 0 and store.pendingSystemRelease() == null);
    try t.expectError(error.Busy, store.endUse(first_mapping.lease, driver, false));
    for (store.leases) |*lease| if (lease.handle.id != 0) try store.endUse(lease.handle, driver, true);
    var freed: usize = 0;
    while (store.pendingSystemRelease()) |ticket| {
        try store.finishRelease(ticket, true);
        freed += 1;
    }
    try t.expect(freed == 513);
    try t.expectEqualDeep(Stats{}, store.stats());
    try t.expectError(error.Stale, store.useInfo(first_mapping.lease, driver));
}

fn deviceBudgetLifetime() !void {
    const t = std.testing;
    var store = Table(8, 12, 4){ .budget_bytes = 8192, .producer_budget_bytes = 4096 };
    const app: Owner = .{ .kind = .program, .id = 1, .generation = 1 };
    const other: Owner = .{ .kind = .program, .id = 2, .generation = 1 };
    const gpu: Owner = .{ .kind = .driver, .id = 3, .generation = 7 };
    const binding: layout.Binding = .{ .adapter = 4, .driver_owner = 3, .device_generation = 9 };
    const descriptor: layout.Descriptor = .{ .bytes = 4096, .usage = 12, .location = .device_local, .binding = binding };
    const cpu = try store.begin(app, .{ .bytes = 4096 });
    const first = try store.beginOwned(gpu, descriptor, 21);
    try t.expectError(error.Budget, store.beginOwned(gpu, descriptor, 22));
    // A newly configured provider adopts its already represented boot BOs.
    // Native capacity is independent of RAM and its per-program budget.
    try store.setDeviceBudget(gpu, binding, 12288);
    var large = descriptor; large.bytes = 8192;
    const second = try store.beginOwned(gpu, large, 22);
    const other_cpu = try store.begin(other, .{ .bytes = 4096 });
    try t.expect(store.totalBudgetBytes() == 20480 and store.stats().bytes == 20480);
    try t.expect((try store.deviceBudget(gpu, binding)).charged == 12288);
    try t.expect(store.sharedChargedBytes() == 8192);
    try t.expectEqualDeep(try store.deviceBudget(gpu, binding), try store.queryDeviceBudget(binding.adapter, binding.device_generation));
    try t.expectError(error.Stale, store.queryDeviceBudget(binding.adapter, binding.device_generation + 1));
    const replacement_driver: Owner = .{ .kind = .driver, .id = 5, .generation = 8 };
    var replacement_binding = binding; replacement_binding.driver_owner = 5;
    try t.expectError(error.Busy, store.setDeviceBudget(replacement_driver, replacement_binding, 8192));
    try t.expectError(error.Budget, store.begin(app, .{ .bytes = 4096 }));
    try store.setDeviceBudget(gpu, binding, 4096);
    try t.expect((try store.deviceBudget(gpu, binding)).charged == 12288 and store.stats().device_bytes == 12288);
    try t.expectError(error.Budget, store.beginOwned(gpu, descriptor, 23));
    var wrong = binding; wrong.device_generation += 1;
    try t.expectError(error.Busy, store.setDeviceBudget(gpu, wrong, 4096));
    try t.expectError(error.Overflow, store.setDeviceBudget(gpu, binding, std.math.maxInt(u64) & ~@as(u64, 4095)));
    try t.expect((try store.deviceBudget(gpu, binding)).limit == 4096);
    try store.abortOwned(second, gpu, true);
    try store.setDeviceBudget(gpu, binding, 8192);
    const replacement = try store.beginOwned(gpu, descriptor, 23);
    store.stoppedOwner(gpu);
    try t.expect((try store.deviceBudget(gpu, binding)).closing);
    try t.expectError(error.Closed, store.setDeviceBudget(gpu, binding, 12288));
    try t.expectError(error.Closed, store.beginOwned(gpu, descriptor, 24));
    try store.abortOwned(first, gpu, true);
    try t.expect((try store.deviceBudget(gpu, binding)).charged == 4096);
    try store.abortOwned(replacement, gpu, true);
    try t.expectError(error.Stale, store.deviceBudget(gpu, binding));
    try t.expect(store.totalBudgetBytes() == 8192);
    try store.abort(cpu); try store.abort(other_cpu);
    try t.expectEqualDeep(Stats{}, store.stats());
}

fn accountingLifetime() !void {
    const t = std.testing;
    var store = Table(4, 12, 12){ .budget_bytes = 16384, .producer_budget_bytes = 16384 };
    const app: Owner = .{ .kind = .program, .id = 3, .generation = 1 };
    const gpu: Owner = .{ .kind = .driver, .id = 4, .generation = 7 };
    const binding: layout.Binding = .{ .adapter = 2, .driver_owner = 4, .device_generation = 9 };
    var raster: layout.Descriptor = .{ .bytes = 1024, .format = .xrgb8888, .width = 16, .height = 16,
        .plane_count = 1, .usage = layout.Usage.cpu_read | layout.Usage.scanout };
    raster.planes[0].pitch = 64;
    const cpu = try store.begin(app, raster);
    const native = try store.beginOwned(gpu, .{ .bytes = 4091, .usage = 12, .location = .device_local, .binding = binding }, 19);
    var stats = store.stats();
    try t.expect(stats.bytes == 8192 and stats.system_bytes == 4096 and stats.device_bytes == 4096 and
        stats.allocating_bytes == 8192 and stats.system_backed_bytes == 0 and stats.device_backed_bytes == 0);
    try store.publish(cpu, .{ .cookie = 21, .bytes = 4096, .cpu_address = 0x10000, .cache = .write_back });
    try store.commitOwned(native, gpu);
    const shared = try store.share(cpu.reference, gpu);
    const a = try store.use(cpu.reference, app, .cpu_read, 0, 1024);
    const b = try store.use(cpu.reference, app, .cpu_read, 0, 1024);
    const scanout = try store.use(shared, gpu, .scanout, 0, 1024);
    const mapping = try store.use(shared, gpu, .device_mapping, 0, 1024);
    const draw = try store.use(native.create.reference, gpu, .device_write, 0, 4091);
    stats = store.stats();
    try t.expect(stats.leases == 5 and stats.references == 3 and stats.system_backed_bytes == 4096 and
        stats.device_backed_bytes == 4096 and stats.system_pinned_bytes == 4096 and stats.device_pinned_bytes == 4096 and
        stats.scanout_pinned_bytes == 4096 and stats.device_mapped_bytes == 4096 and stats.allocating_bytes == 0);
    store.stoppedOwner(app);
    try t.expectError(error.Stale, store.useInfo(a.lease, app));
    try t.expectError(error.Stale, store.useInfo(b.lease, app));
    stats = store.stats();
    try t.expect(stats.leases == 3 and stats.system_pinned_bytes == 4096 and stats.retained_bytes == 4096);
    try store.drop(shared, gpu);
    try store.drop(native.create.reference, gpu);
    try t.expectError(error.Busy, store.endUse(draw.lease, gpu, false));
    try t.expect(store.stats().device_pinned_bytes == 4096 and store.pendingRelease() == null);
    try store.endUse(scanout.lease, gpu, true);
    try t.expect(store.stats().scanout_pinned_bytes == 0 and store.stats().system_pinned_bytes == 4096);
    try store.endUse(mapping.lease, gpu, true);
    try store.endUse(draw.lease, gpu, true);
    stats = store.stats();
    try t.expect(stats.system_pinned_bytes == 0 and stats.device_pinned_bytes == 0 and
        stats.device_mapped_bytes == 0 and stats.destroying_bytes == 8192);
    const cpu_release = store.pendingSystemRelease().?;
    try t.expectError(error.Busy, store.finishRelease(cpu_release, false));
    const native_release = (try store.takeOwnedRelease(gpu, binding)).?;
    try t.expectError(error.Busy, store.finishOwnedRelease(native_release, gpu, false));
    try t.expect(store.stats().system_backed_bytes == 4096 and store.stats().device_backed_bytes == 4096);
    try store.finishRelease(store.pendingSystemRelease().?, true);
    try store.finishOwnedRelease(native_release, gpu, true);
    try t.expectEqualDeep(Stats{}, store.stats());
}

fn ownedBackingLifetime(native_layout: bool) !void {
    const t = std.testing;
    var store = Table(4, 8, 8){ .budget_bytes = 16384, .producer_budget_bytes = 16384 };
    const driver: Owner = .{ .kind = .driver, .id = 7, .generation = 4 };
    const stale: Owner = .{ .kind = .driver, .id = 7, .generation = 5 };
    const app: Owner = .{ .kind = .program, .id = 8, .generation = 1 };
    const binding: layout.Binding = .{ .adapter = 2, .driver_owner = 7, .device_generation = 9 };
    var desc: layout.Descriptor = .{ .bytes = 4091, .location = .device_local, .binding = binding, .usage = 12 };
    if (native_layout) {
        desc.modifier = 0x0300000000606010; desc.width = 5; desc.height = 3; desc.format = .nv12; desc.plane_count = 2;
        desc.planes[0] = .{ .pitch = 64 }; desc.planes[1] = .{ .offset = 2048, .pitch = 64 };
        try t.expectError(error.Unsupported, store.begin(driver, desc));
    }
    const reserved = try store.beginOwned(driver, desc, 0x100000001);
    try t.expect(store.retainsDriver(driver));
    try t.expect(!store.retainsDriver(stale));
    try t.expectError(error.Closed, store.share(reserved.create.reference, app));
    try t.expectError(error.Busy, store.beginOwned(driver, desc, reserved.cookie));
    try t.expectError(error.WrongOwner, store.commitOwned(reserved, stale));
    var forged = reserved;
    forged.create.bytes -= 1;
    try t.expectError(error.Stale, store.abortOwned(forged, driver, true));
    forged = reserved; forged.cookie += 1;
    try t.expectError(error.Stale, store.commitOwned(forged, driver));
    try t.expectError(error.Busy, store.abortOwned(reserved, driver, false));
    try store.commitOwned(reserved, driver);
    try t.expectError(error.Stale, store.commitOwned(reserved, driver));
    try t.expectError(error.Unsupported, store.use(reserved.create.reference, driver, .cpu_read, 0, 1));
    const imported = try store.share(reserved.create.reference, app);
    try t.expect(std.meta.eql(desc, try store.describe(imported, app)));
    const device = try store.use(reserved.create.reference, driver, .device_write, 0, 4091);
    try store.drop(reserved.create.reference, driver);
    try t.expectEqual(@as(?OwnedRelease, null), try store.takeOwnedRelease(driver, binding));
    try store.drop(imported, app);
    try t.expectEqual(@as(?OwnedRelease, null), try store.takeOwnedRelease(driver, binding));
    try t.expectError(error.Busy, store.endUse(device.lease, driver, false));
    try store.endUse(device.lease, driver, true);
    // A native object in the first slot cannot stall unrelated VM teardown.
    const cpu = try store.begin(app, .{ .bytes = 4096 });
    try store.publish(cpu, .{ .cookie = 88, .bytes = 4096 });
    try store.drop(cpu.reference, app);
    const cpu_release = store.pendingSystemRelease().?;
    try t.expect(cpu_release.buffer.eql(cpu.buffer));
    try store.finishRelease(cpu_release, true);
    try t.expectEqual(@as(?Release, null), store.pendingSystemRelease());
    var wrong_binding = binding; wrong_binding.device_generation += 1;
    try t.expectEqual(@as(?OwnedRelease, null), try store.takeOwnedRelease(driver, wrong_binding));
    try t.expectEqual(@as(?OwnedRelease, null), try store.takeOwnedRelease(stale, binding));
    const release = (try store.takeOwnedRelease(driver, binding)).?;
    try t.expectEqual(@as(?OwnedRelease, null), try store.takeOwnedRelease(driver, binding));
    try t.expectError(error.Busy, store.finishOwnedRelease(release, driver, false));
    var bad = release; bad.release.backing.bytes += 4096;
    try t.expectError(error.Stale, store.finishOwnedRelease(bad, driver, true));
    bad = release; bad.release.attempt += 1;
    try t.expectError(error.Stale, store.finishOwnedRelease(bad, driver, true));
    try t.expectError(error.WrongOwner, store.finishOwnedRelease(release, stale, true));
    try t.expectEqual(@as(u64, 4096), store.stats().bytes);
    try store.finishOwnedRelease(release, driver, true);
    try t.expectError(error.Stale, store.finishOwnedRelease(release, driver, true));
    const incomplete = try store.beginOwned(driver, desc, reserved.cookie);
    store.stoppedOwner(driver);
    try t.expect(store.retainsDriver(driver));
    try t.expect(store.pendingReleaseForOwner(driver));
    try t.expectError(error.Closed, store.commitOwned(incomplete, driver));
    try store.abortOwned(incomplete, driver, true);
    try t.expect(!store.retainsDriver(driver));
    try t.expectEqual(@as(u64, 0), store.stats().bytes);
}

fn resetBackingLifetime() !void {
    const t = std.testing;
    var store = Table(8, 12, 8){ .budget_bytes = 8192, .producer_budget_bytes = 8192 };
    const driver: Owner = .{ .kind = .driver, .id = 7, .generation = 3 };
    const app: Owner = .{ .kind = .program, .id = 8, .generation = 4 };
    const binding: layout.Binding = .{ .adapter = 2, .driver_owner = 7, .device_generation = 9 };
    try store.setDeviceBudget(driver, binding, 8192);
    const cpu = try store.begin(app, .{ .bytes = 4096 });
    try store.publish(cpu, .{ .cookie = 55, .bytes = 4096 });
    const native = try store.beginOwned(driver, .{ .bytes = 4096, .location = .device_local, .binding = binding, .usage = 12 }, 77);
    try store.commitOwned(native, driver);
    const imported = try store.share(native.create.reference, app);
    const device = try store.use(native.create.reference, driver, .device_write, 0, 4096);
    try t.expectError(error.WrongOwner, store.loseDevice(.{ .kind = .driver, .id = 9, .generation = 3 }, binding, true));
    try store.loseDevice(driver, binding, false);
    try t.expectError(error.Closed, store.describe(imported, app));
    try t.expectError(error.Closed, store.share(imported, app));
    try t.expectError(error.Closed, store.use(imported, app, .device_write, 0, 4096));
    try t.expectEqual(@as(u64, 4096), (try store.describe(cpu.reference, app)).bytes);
    try t.expect((try store.deviceBudget(driver, binding)).closing);
    var next = binding; next.device_generation += 1;
    try t.expectError(error.Busy, store.setDeviceBudget(driver, next, 8192));
    try t.expectError(error.Busy, store.endUse(device.lease, driver, false));
    try t.expect((try store.takeOwnedRelease(driver, binding)) == null);
    try store.loseDevice(driver, binding, true);
    try t.expect((try store.takeOwnedRelease(driver, binding)) == null); // actual DMA owner still holds its lease
    _ = try store.useInfo(device.lease, driver); // cleanup remains possible
    try store.endUse(device.lease, driver, true);
    const ticket = (try store.takeOwnedRelease(driver, binding)).?;
    try t.expectError(error.Busy, store.finishOwnedRelease(ticket, driver, false));
    try t.expectEqual(@as(u64, 8192), store.stats().bytes);
    try store.finishOwnedRelease(ticket, driver, true);
    try t.expectError(error.Stale, store.finishOwnedRelease(ticket, driver, true));
    try t.expectEqual(@as(u64, 4096), store.stats().bytes);
    try t.expect(!store.retainsDriver(driver));
    try store.setDeviceBudget(driver, next, 8192); // old reference stubs do not retain physical VRAM budget
    const replacement = try store.beginOwned(driver, .{ .bytes = 4096, .location = .device_local, .binding = next, .usage = 12 }, 77);
    try store.commitOwned(replacement, driver);
    try t.expect(!replacement.create.buffer.eql(native.create.buffer));
    try t.expectError(error.Closed, store.describe(imported, app));
    try store.loseDevice(driver, binding, false); // a late old-generation notification cannot revive or affect new storage
    try t.expectEqual(@as(u64, 4096), (try store.describe(replacement.create.reference, driver)).bytes);
    try store.drop(imported, app);
    try store.drop(native.create.reference, driver);
    try store.drop(replacement.create.reference, driver);
    try store.finishOwnedRelease((try store.takeOwnedRelease(driver, next)).?, driver, true);
    try store.drop(cpu.reference, app);
    try store.finishRelease(store.pendingSystemRelease().?, true);
    try t.expectEqualDeep(Stats{}, store.stats());
}

pub const Pool = enum { objects, references, leases };
/// Production metadata grows outside the enclosing no-sleep owner. Handles
/// survive replacement; internal record pointers never escape that owner.
pub fn DynamicTable() type { return StorageTable(null, null, null); }
/// Fixed backing remains useful for bounded lifetime/fault models.
pub fn Table(comptime objects: usize, comptime references: usize, comptime leases: usize) type {
    return StorageTable(objects, references, leases);
}
fn Slice(comptime SelfPointer: type, comptime Element: type) type {
    return if (@typeInfo(SelfPointer).pointer.is_const) []const Element else []Element;
}
fn StorageTable(comptime object_capacity: ?usize, comptime reference_capacity: ?usize, comptime lease_capacity: ?usize) type {
    comptime {
        for ([_]?usize{object_capacity, reference_capacity, lease_capacity}) |capacity|
            if (capacity != null and capacity.? > std.math.maxInt(u32)) @compileError("buffer handle capacity exceeds u32");
    }
    return struct {
        const Self = @This();
        const Phase = enum { empty, allocating, live, releasing, destroying };
        const Object = struct {
            phase: Phase = .empty,
            handle: Handle = .{},
            producer: Owner = .{ .kind = .kernel, .id = 0, .generation = 0 },
            producer_open: bool = true,
            descriptor: layout.Descriptor = .{ .bytes = 0 },
            allocation_bytes: u64 = 0,
            backing: ?Backing = null,
            references: u32 = 0,
            leases: u32 = 0,
            scanout_leases: u32 = 0,
            mapping_leases: u32 = 0,
            release_attempt: u64 = 0,
            owned_cookie: u64 = 0,
            owned_reference: Handle = .{},
            device_lost: bool = false,
            device_quiesced: bool = false,
        };
        const Reference = struct { handle: Handle = .{}, buffer: Handle = .{}, owner: Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, read_only: bool = false, mapping_only: bool = false };
        const Lease = struct { handle: Handle = .{}, buffer: Handle = .{}, owner: Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, access: Access = .cpu_read, range: layout.Range = .{ .offset = 0, .bytes = 0 } };
        const DeviceBudget = struct {
            driver: Owner,
            binding: layout.Binding,
            bytes: u64,
            closing: bool = false,
        };

        objects: if (object_capacity) |n| [n]Object else []Object = if (object_capacity != null) @splat(Object{}) else &.{},
        references: if (reference_capacity) |n| [n]Reference else []Reference = if (reference_capacity != null) @splat(Reference{}) else &.{},
        leases: if (lease_capacity) |n| [n]Lease else []Lease = if (lease_capacity != null) @splat(Lease{}) else &.{},
        serial: u64 = 0,
        committed_bytes: u64 = 0,
        budget_bytes: u64,
        producer_budget_bytes: u64,
        device_budgets: [@min(object_capacity orelse 16, 16)]?DeviceBudget = @splat(null),

        pub fn Element(comptime kind: Pool) type {
            return switch (kind) { .objects => Object, .references => Reference, .leases => Lease };
        }
        fn objectsSlice(self: anytype) Slice(@TypeOf(self), Object) {
            return if (object_capacity != null) &self.objects else self.objects;
        }
        fn referencesSlice(self: anytype) Slice(@TypeOf(self), Reference) {
            return if (reference_capacity != null) &self.references else self.references;
        }
        fn leasesSlice(self: anytype) Slice(@TypeOf(self), Lease) {
            return if (lease_capacity != null) &self.leases else self.leases;
        }
        pub fn freeSlots(self: *const Self, comptime kind: Pool, needed: usize) usize {
            const values = switch (kind) { .objects => self.objectsSlice(), .references => self.referencesSlice(), .leases => self.leasesSlice() };
            var count: usize = 0;
            for (values) |*value| if (value.handle.id == 0) {
                count += 1;
                if (count >= needed) break;
            };
            return count;
        }
        /// Caller allocated resident replacement before taking the metadata
        /// owner. Return the detached old storage for release after unlocking.
        /// The caller initializes the new tail to default records outside the
        /// owner; the existing prefix is copied from the latest locked state.
        /// No record pointer may be retained across this owner boundary.
        pub fn replaceStorage(self: *Self, comptime kind: Pool, prepared: []Element(kind)) Error![]Element(kind) {
            const capacity = switch (kind) { .objects => object_capacity, .references => reference_capacity, .leases => lease_capacity };
            if (capacity != null) return error.Unsupported;
            const old = @field(self, @tagName(kind));
            if (prepared.len <= old.len or prepared.len > std.math.maxInt(u32)) return error.Invalid;
            const start = @intFromPtr(prepared.ptr);
            const end = start + std.mem.sliceAsBytes(prepared).len;
            const previous = @intFromPtr(old.ptr);
            if (old.len != 0 and start < previous + std.mem.sliceAsBytes(old).len and previous < end) return error.Invalid;
            @memcpy(prepared[0..old.len], old);
            @field(self, @tagName(kind)) = prepared;
            return old;
        }

        fn belongs(object: *const Object, budget: DeviceBudget) bool {
            return object.phase != .empty and object.descriptor.location == .device_local and
                object.producer.eql(budget.driver) and std.meta.eql(object.descriptor.binding, budget.binding);
        }
        fn budgetIndex(self: *const Self, producer: Owner, descriptor: layout.Descriptor) ?usize {
            if (descriptor.location != .device_local or producer.kind != .driver) return null;
            for (&self.device_budgets, 0..) |*slot, index| if (slot.*) |budget| {
                if (budget.driver.eql(producer) and std.meta.eql(budget.binding, descriptor.binding)) return index;
            };
            return null;
        }
        fn chargedDevice(self: *const Self, budget: DeviceBudget) u64 {
            var bytes: u64 = 0;
            for (self.objectsSlice()) |*object| if (belongs(object, budget)) { bytes += object.allocation_bytes; };
            return bytes;
        }
        /// Generic accounting limit supplied by the authenticated memory
        /// provider. It grants no physical allocation or placement policy.
        /// Live charges may exceed a reduced limit; new admission then waits
        /// for confirmed retirement instead of forgetting existing backing.
        pub fn setDeviceBudget(self: *Self, driver: Owner, binding: layout.Binding, bytes: u64) Error!void {
            if (!driver.valid() or driver.kind != .driver or driver.id > std.math.maxInt(u32) or
                !binding.valid() or binding.portable() or binding.driver_owner != driver.id or bytes % 4096 != 0) return error.Invalid;
            var selected: ?usize = null;
            var empty: ?usize = null;
            var combined = self.budget_bytes;
            for (&self.device_budgets, 0..) |*slot, index| {
                const budget = slot.* orelse { if (empty == null) empty = index; continue; };
                if (budget.driver.eql(driver) and std.meta.eql(budget.binding, binding)) {
                    if (budget.closing) return error.Closed;
                    selected = index;
                    continue;
                }
                // An adapter has one physical admission domain. A new driver
                // epoch must not double-budget backing retained by the old one.
                if (budget.binding.adapter == binding.adapter) return error.Busy;
                combined = std.math.add(u64, combined, budget.bytes) catch return error.Overflow;
            }
            _ = std.math.add(u64, combined, bytes) catch return error.Overflow;
            const index = selected orelse empty orelse return error.Capacity;
            self.device_budgets[index] = .{ .driver = driver, .binding = binding, .bytes = bytes };
        }
        pub fn deviceBudget(self: *const Self, driver: Owner, binding: layout.Binding) Error!BudgetSnapshot {
            for (&self.device_budgets) |*slot| if (slot.*) |budget| {
                if (budget.driver.eql(driver) and std.meta.eql(budget.binding, binding))
                    return .{ .limit = budget.bytes, .charged = self.chargedDevice(budget), .closing = budget.closing };
            };
            return error.Stale;
        }
        pub fn queryDeviceBudget(self: *const Self, adapter: u32, generation: u64) Error!BudgetSnapshot {
            if (adapter == 0 or generation == 0) return error.Invalid;
            for (&self.device_budgets) |*slot| if (slot.*) |budget| {
                if (budget.binding.adapter == adapter and budget.binding.device_generation == generation)
                    return .{ .limit = budget.bytes, .charged = self.chargedDevice(budget), .closing = budget.closing };
            };
            return error.Stale;
        }
        pub fn sharedChargedBytes(self: *const Self) u64 {
            var bytes = self.committed_bytes;
            for (&self.device_budgets) |*slot| if (slot.*) |budget| { bytes -= self.chargedDevice(budget); };
            return bytes;
        }
        pub fn totalBudgetBytes(self: *const Self) u64 {
            var bytes = self.budget_bytes;
            for (&self.device_budgets) |*slot| if (slot.*) |budget| { bytes += budget.bytes; };
            return bytes; // Overflow was checked before configuration publication.
        }
        fn collectClosedBudgets(self: *Self) void {
            for (&self.device_budgets) |*slot| if (slot.*) |budget| {
                if (budget.closing and self.chargedDevice(budget) == 0) slot.* = null;
            };
        }

        pub fn begin(self: *Self, producer: Owner, descriptor: layout.Descriptor) Error!Create {
            return self.beginValidated(producer, descriptor, try layout.validate(descriptor));
        }
        fn beginValidated(self: *Self, producer: Owner, descriptor: layout.Descriptor, validated: layout.Layout) Error!Create {
            if (!producer.valid()) return error.Invalid;
            const selected = self.budgetIndex(producer, descriptor);
            var shared_bytes: u64 = 0;
            var owner_bytes: u64 = 0;
            for (self.objectsSlice()) |object| {
                if (object.phase == .empty or self.budgetIndex(object.producer, object.descriptor) != null) continue;
                shared_bytes += object.allocation_bytes;
                if (object.producer.eql(producer)) owner_bytes += object.allocation_bytes;
            }
            if (selected) |index| {
                const budget = self.device_budgets[index].?;
                if (budget.closing) return error.Closed;
                const charged = self.chargedDevice(budget);
                if (charged > budget.bytes or validated.allocation_bytes > budget.bytes - charged) return error.Budget;
            } else {
                if (shared_bytes > self.budget_bytes or validated.allocation_bytes > self.budget_bytes - shared_bytes) return error.Budget;
                if (owner_bytes > self.producer_budget_bytes or validated.allocation_bytes > self.producer_budget_bytes - owner_bytes) return error.Budget;
            }
            _ = std.math.add(u64, self.committed_bytes, validated.allocation_bytes) catch return error.Overflow;
            const slot = self.freeObject() orelse return error.Capacity;
            const reference_slot = self.freeReference() orelse return error.Capacity;
            // Reserve both serials before publication, so failure has no live
            // partially initialized reference or charged memory.
            if (self.serial > std.math.maxInt(u64) - 2) return error.Exhausted;
            const handle = try self.nextHandle(slot);
            const reference = try self.nextHandle(reference_slot);
            self.objects[slot] = .{ .phase = .allocating, .handle = handle, .producer = producer, .descriptor = descriptor, .allocation_bytes = validated.allocation_bytes, .references = 1 };
            self.references[reference_slot] = .{ .handle = reference, .buffer = handle, .owner = producer };
            self.committed_bytes += validated.allocation_bytes;
            return .{ .buffer = handle, .reference = reference, .bytes = validated.allocation_bytes };
        }

        // Reserve common lifetime/budget before the driver starts allocating.
        // This cookie is opaque; it is never interpreted as a VM or GPU address.
        pub fn beginOwned(self: *Self, driver: Owner, descriptor: layout.Descriptor, cookie: u64) Error!OwnedCreate {
            if (!driver.valid() or driver.kind != .driver or driver.id > std.math.maxInt(u32) or cookie == 0) return error.Invalid;
            if (descriptor.location != .device_local or descriptor.binding.portable() or
                descriptor.binding.driver_owner != driver.id or
                descriptor.usage & (layout.Usage.cpu_read | layout.Usage.cpu_write) != 0) return error.Unsupported;
            for (self.objectsSlice()) |object| {
                if (object.phase != .empty and object.producer.eql(driver) and object.owned_cookie == cookie and
                    std.meta.eql(object.descriptor.binding, descriptor.binding)) return error.Busy;
            }
            const ticket = try self.beginValidated(driver, descriptor, try layout.validateDriverOwned(descriptor));
            const object = try self.findObject(ticket.buffer);
            object.owned_cookie = cookie;
            object.owned_reference = ticket.reference;
            return .{ .create = ticket, .cookie = cookie, .driver = driver, .binding = descriptor.binding };
        }
        fn ownedCreation(self: *Self, ticket: OwnedCreate, driver: Owner) Error!*Object {
            if (!ticket.driver.eql(driver)) return error.WrongOwner;
            const object = try self.findObject(ticket.create.buffer);
            if (!object.producer.eql(driver)) return error.WrongOwner;
            if (object.phase != .allocating or object.owned_cookie == 0 or object.owned_cookie != ticket.cookie or
                !object.owned_reference.eql(ticket.create.reference) or object.allocation_bytes != ticket.create.bytes or
                !std.meta.eql(object.descriptor.binding, ticket.binding)) return error.Stale;
            return object;
        }
        pub fn commitOwned(self: *Self, ticket: OwnedCreate, driver: Owner) Error!void {
            const object = try self.ownedCreation(ticket, driver);
            if (!object.producer_open or object.references == 0) return error.Closed;
            try self.publish(ticket.create, .{ .cookie = ticket.cookie, .bytes = ticket.create.bytes, .driver = driver });
        }
        pub fn abortOwned(self: *Self, ticket: OwnedCreate, driver: Owner, quiesced: bool) Error!void {
            _ = try self.ownedCreation(ticket, driver);
            if (!quiesced) return error.Busy;
            try self.abort(ticket.create);
        }

        pub fn publish(self: *Self, ticket: Create, backing: Backing) Error!void {
            const object = try self.findObject(ticket.buffer);
            if (object.phase != .allocating or object.backing != null) return error.Stale;
            if (backing.cookie == 0 or backing.bytes < object.allocation_bytes or
                (backing.cpu_address == 0) != (backing.cache == .unavailable) or
                (backing.cpu_address != 0 and (backing.cpu_address % object.descriptor.alignment != 0 or
                    backing.cpu_address > std.math.maxInt(u64) - backing.bytes))) return error.Invalid;
            if (backing.driver) |driver| {
                if (driver.kind != .driver or !driver.valid() or object.descriptor.binding.driver_owner != driver.id) return error.Invalid;
            } else if (object.descriptor.location == .device_local) return error.Invalid;
            object.backing = backing;
            // A stopped producer can disappear while the allocator prepares
            // pages. The result remains represented for deferred release.
            object.phase = if (object.references == 0 and object.leases == 0) .releasing else .live;
        }

        pub fn abort(self: *Self, ticket: Create) Error!void {
            const object = try self.findObject(ticket.buffer);
            if (object.phase != .allocating or object.backing != null) return error.Stale;
            for (self.referencesSlice()) |*reference| if (reference.buffer.eql(ticket.buffer)) {
                reference.* = .{};
            };
            self.committed_bytes -= object.allocation_bytes;
            object.* = .{};
            self.collectClosedBudgets();
        }

        pub fn import(self: *Self, buffer: Handle, consumer: Owner) Error!Handle {
            return self.importMode(buffer, consumer, false);
        }

        pub fn importMode(self: *Self, buffer: Handle, consumer: Owner, read_only: bool) Error!Handle {
            if (!consumer.valid()) return error.Invalid;
            const object = try self.findObject(buffer);
            if (object.phase != .live or !object.producer_open) return error.Closed;
            // An immutable export cannot be published while a writer still
            // holds access, including a persistent Vulkan CPU mapping.
            if (read_only) for (self.leasesSlice()) |lease| {
                if (lease.handle.id != 0 and lease.buffer.eql(buffer) and writes(lease.access)) return error.Busy;
            };
            const slot = self.freeReference() orelse return error.Capacity;
            const reference = try self.nextHandle(slot);
            self.references[slot] = .{ .handle = reference, .buffer = buffer, .owner = consumer, .read_only = read_only };
            object.references += 1;
            return reference;
        }

        pub fn describe(self: *Self, reference: Handle, owner: Owner) Error!layout.Descriptor {
            const object = try self.referencedObject(reference, owner);
            if (object.device_lost) return error.Closed;
            return object.descriptor;
        }

        // Sharing names a live reference, preserving immutable-export mode.
        // A bare diagnostic object ID is not an importable reference.
        pub fn share(self: *Self, handle: Handle, consumer: Owner) Error!Handle {
            if (handle.id == 0 or handle.id > self.references.len or handle.generation == 0) return error.Stale;
            const item = self.references[handle.id - 1];
            if (!item.handle.eql(handle)) return error.Stale;
            if (item.mapping_only) return error.Unsupported;
            return self.importMode(item.buffer, consumer, item.read_only);
        }

        pub fn readOnly(self: *Self, handle: Handle, owner: Owner) Error!bool {
            return (try self.findReference(handle, owner)).read_only;
        }

        pub fn mappingOnly(self: *Self, handle: Handle, owner: Owner) Error!bool {
            return (try self.findReference(handle, owner)).mapping_only;
        }

        pub fn drop(self: *Self, handle: Handle, owner: Owner) Error!void {
            const reference = try self.findReference(handle, owner);
            const object = try self.findObject(reference.buffer);
            reference.* = .{};
            object.references -= 1;
            self.maybeRelease(object);
        }

        pub fn use(self: *Self, reference: Handle, owner: Owner, access: Access, offset: u64, bytes: u64) Error!Use {
            const ref_record = try self.findReference(reference, owner);
            if (ref_record.mapping_only and access != .device_mapping) return error.Unsupported;
            if (ref_record.read_only and writes(access)) return error.Unsupported;
            const object = try self.referencedObject(reference, owner);
            if (object.phase != .live or object.device_lost) return error.Closed;
            if (writes(access)) {
                for (self.referencesSlice()) |item| if (item.handle.id != 0 and item.buffer.eql(object.handle) and item.read_only) {
                    return error.Busy;
                };
            }
            // Native address translation covers complete allocated pages.
            // Padding remains inaccessible to CPU/execution/queue users;
            // only the mapping-only reference may retain that backing tail.
            const limit = if (ref_record.mapping_only) object.allocation_bytes else object.descriptor.bytes;
            if (!layout.spanFits(limit, offset, bytes)) return error.Invalid;
            const backing = object.backing orelse return error.Busy;
            const required: u32 = switch (access) {
                .cpu_read, .cpu_persistent_read => layout.Usage.cpu_read,
                .cpu_write => layout.Usage.cpu_write,
                .cpu_persistent_write => layout.Usage.cpu_read | layout.Usage.cpu_write,
                .device_read, .queue_read => layout.Usage.transfer_source,
                .device_write, .queue_write => layout.Usage.transfer_target | layout.Usage.render,
                .scanout => layout.Usage.scanout,
                .device_mapping => 0,
            };
            if (required != 0 and (object.descriptor.usage & required) == 0) return error.Unsupported;
            if (persistentCpu(access) and (object.descriptor.location != .system or backing.cache != .write_back or
                object.descriptor.usage & required != required)) return error.Unsupported;
            if (isCpu(access) and backing.cpu_address == 0) return error.Unsupported;
            if (access == .cpu_read and backing.cache == .write_combining) return error.Unsupported;
            for (self.leasesSlice()) |lease| {
                if (lease.handle.id != 0 and lease.buffer.eql(object.handle) and conflicts(lease.access, access)) return error.Busy;
            }
            const slot = self.freeLease() orelse return error.Capacity;
            const token = try self.nextHandle(slot);
            self.leases[slot] = .{ .handle = token, .buffer = object.handle, .owner = owner, .access = access, .range = .{ .offset = offset, .bytes = bytes } };
            object.leases += 1;
            if (access == .scanout) object.scanout_leases += 1;
            if (access == .device_mapping) object.mapping_leases += 1;
            return .{ .lease = token, .buffer = object.handle, .access = access, .backing = backing, .range = .{ .offset = offset, .bytes = bytes } };
        }

        // The queue owner must first validate ordering against every existing
        // conflicting queue use while holding this same metadata lock. Queued
        // uses may overlap each other, but exclude conflicting CPU/device maps.
        // Ownership changes atomically with admission, before producer cleanup.
        pub fn reserveQueued(self: *Self, reference: Handle, producer: Owner, queue_owner: Owner, write: bool, offset: u64, bytes: u64) Error!Use {
            if (queue_owner.kind != .kernel or !queue_owner.valid()) return error.Invalid;
            const result = try self.use(reference, producer, if (write) .queue_write else .queue_read, offset, bytes);
            const lease = try self.findLease(result.lease, producer);
            lease.owner = queue_owner;
            return result;
        }

        // Only a validated, active native job may hand its existing queue
        // lease to a driver. Producer exit closes public imports, but cannot
        // revoke backing already held by that job. The new reference retains
        // the whole BO for address translation, never for CPU access, sharing
        // or execution. Those permissions and extents remain with queue uses.
        pub fn retainQueued(self: *Self, handle: Handle, queue_owner: Owner, driver: Owner) Error!Handle {
            if (queue_owner.kind != .kernel or !queue_owner.valid() or driver.kind != .driver or !driver.valid()) return error.Invalid;
            const lease = try self.findLease(handle, queue_owner);
            if (!queued(lease.access)) return error.Unsupported;
            const object = try self.findObject(lease.buffer);
            if (object.phase != .live or object.device_lost or object.backing == null) return error.Closed;
            const slot = self.freeReference() orelse return error.Capacity;
            const reference = try self.nextHandle(slot);
            self.references[slot] = .{ .handle = reference, .buffer = object.handle, .owner = driver, .mapping_only = true };
            object.references += 1;
            return reference;
        }

        /// Direct presentation retains a read-only full reference so the
        /// display owner can acquire its own real device-read lease. Its
        /// presence excludes new writers until final DMA-context retirement.
        pub fn retainScanout(self: *Self, handle: Handle, queue_owner: Owner, driver: Owner) Error!Handle {
            const lease = try self.findLease(handle, queue_owner);
            const object = try self.findObject(lease.buffer);
            if (lease.access != .queue_read or object.descriptor.location != .device_local or
                object.descriptor.usage & layout.Usage.scanout == 0) return error.Unsupported;
            const reference = try self.retainQueued(handle, queue_owner, driver);
            const retained = self.findReference(reference, driver) catch unreachable;
            retained.mapping_only = false; retained.read_only = true;
            return reference;
        }

        // GPU/DMA/scanout leases need an engine/TLB completion or a proven
        // stop. A timeout, cancel request, or producer death is not that proof.
        pub fn endUse(self: *Self, handle: Handle, owner: Owner, device_quiesced: bool) Error!void {
            const lease = try self.findLease(handle, owner);
            if (isDevice(lease.access) and !device_quiesced) return error.Busy;
            const object = try self.findObject(lease.buffer);
            if (lease.access == .scanout) object.scanout_leases -= 1;
            if (lease.access == .device_mapping) object.mapping_leases -= 1;
            lease.* = .{};
            object.leases -= 1;
            self.maybeRelease(object);
        }

        pub fn useInfo(self: *Self, handle: Handle, owner: Owner) Error!Use {
            const lease = try self.findLease(handle, owner);
            const object = try self.findObject(lease.buffer);
            return .{ .lease = handle, .buffer = object.handle, .access = lease.access, .backing = object.backing orelse return error.Busy, .range = lease.range };
        }

        pub fn bufferFor(self: *Self, handle: Handle, owner: Owner) Error!Handle {
            return (try self.findReference(handle, owner)).buffer;
        }

        /// Exact adapter/memory epoch only. Logical loss stops new access but
        /// does not revoke existing DMA leases. After an independently proven
        /// stop, the driver can retire native backing even if applications
        /// still hold invalid references; those references remain closeable.
        pub fn loseDevice(self: *Self, driver: Owner, binding: layout.Binding, quiesced: bool) Error!void {
            if (!driver.valid() or driver.kind != .driver or !binding.valid() or binding.portable()) return error.Invalid;
            if (binding.driver_owner != driver.id) return error.WrongOwner;
            for (&self.device_budgets) |*slot| if (slot.*) |*budget| {
                if (budget.driver.eql(driver) and std.meta.eql(budget.binding, binding)) budget.closing = true;
            };
            for (self.objectsSlice()) |*object| {
                if (object.phase == .empty or object.descriptor.location != .device_local or
                    !object.producer.eql(driver) or !std.meta.eql(object.descriptor.binding, binding)) continue;
                object.device_lost = true;
                object.device_quiesced = object.device_quiesced or quiesced;
                object.producer_open = false;
                self.maybeRelease(object);
            }
            self.collectClosedBudgets();
        }

        // Called only after the program lifecycle has stopped every CPU task.
        // Imported references of other consumers and all device uses survive.
        pub fn stoppedOwner(self: *Self, owner: Owner) void {
            for (&self.device_budgets) |*slot| if (slot.*) |*budget| {
                if (budget.driver.eql(owner)) budget.closing = true;
            };
            for (self.objectsSlice()) |*object| {
                if (object.phase != .empty and object.producer.eql(owner)) object.producer_open = false;
            }
            for (self.referencesSlice()) |*reference| {
                if (reference.handle.id == 0 or !reference.owner.eql(owner)) continue;
                const object = self.findObject(reference.buffer) catch unreachable;
                reference.* = .{};
                object.references -= 1;
            }
            for (self.leasesSlice()) |*lease| {
                if (lease.handle.id == 0 or !lease.owner.eql(owner) or isDevice(lease.access)) continue;
                const object = self.findObject(lease.buffer) catch unreachable;
                lease.* = .{};
                object.leases -= 1;
            }
            for (self.objectsSlice()) |*object| if (object.phase != .empty) {
                self.maybeRelease(object);
            };
            self.collectClosedBudgets();
        }

        // Returning a ticket does not release its budget or slot. The caller
        // must acknowledge successful VM/TLB or backend destruction outside
        // the metadata lock. Failure remains discoverable and retryable.
        pub fn pendingReleaseForOwner(self: *const Self, producer: Owner) bool {
            for (self.objectsSlice()) |object| {
                if ((object.phase == .releasing or object.phase == .destroying) and object.producer.eql(producer)) return true;
                if (object.phase == .allocating and object.owned_cookie != 0 and object.producer.eql(producer)) return true;
            }
            return false;
        }

        pub fn pendingRelease(self: *Self) ?Release {
            return self.pendingFiltered(null, false);
        }
        pub fn pendingSystemRelease(self: *Self) ?Release {
            return self.pendingFiltered(null, true);
        }
        pub fn takeOwnedRelease(self: *Self, driver: Owner, binding: layout.Binding) Error!?OwnedRelease {
            if (!driver.valid() or driver.kind != .driver or !binding.valid() or binding.portable() or binding.driver_owner != driver.id) return error.Invalid;
            const filter: OwnedRelease = .{ .release = undefined, .driver = driver, .binding = binding };
            const ticket = self.pendingFiltered(filter, false) orelse return null;
            return .{ .release = ticket, .driver = driver, .binding = binding };
        }
        fn pendingFiltered(self: *Self, owned: ?OwnedRelease, system_only: bool) ?Release {
            for (self.objectsSlice()) |*object| if (object.phase == .releasing) {
                const backing = object.backing orelse continue;
                if (system_only and backing.driver != null) continue;
                if (owned) |filter| {
                    const driver = backing.driver orelse continue;
                    if (object.owned_cookie == 0 or !driver.eql(filter.driver) or !std.meta.eql(object.descriptor.binding, filter.binding)) continue;
                }
                if (object.release_attempt == std.math.maxInt(u64)) continue;
                object.release_attempt += 1;
                object.phase = .destroying;
                return .{ .buffer = object.handle, .backing = backing, .attempt = object.release_attempt };
            };
            return null;
        }

        pub fn finishOwnedRelease(self: *Self, ticket: OwnedRelease, driver: Owner, quiesced: bool) Error!void {
            if (!ticket.driver.eql(driver)) return error.WrongOwner;
            const object = try self.findObject(ticket.release.buffer);
            const backing = object.backing orelse return error.Stale;
            if (backing.driver == null or !backing.driver.?.eql(driver)) return error.WrongOwner;
            if (object.owned_cookie == 0 or !std.meta.eql(object.descriptor.binding, ticket.binding) or
                !std.meta.eql(backing, ticket.release.backing) or object.release_attempt != ticket.release.attempt or
                object.phase != .destroying) return error.Stale;
            // Unlike the CPU retry collector, an unconfirmed hardware release
            // keeps this exact claimed ticket. It cannot be issued twice.
            if (!quiesced) return error.Busy;
            try self.finishRelease(ticket.release, true);
        }

        pub fn finishRelease(self: *Self, ticket: Release, released: bool) Error!void {
            const object = try self.findObject(ticket.buffer);
            if (object.release_attempt != ticket.attempt) return error.Stale;
            if (object.phase != .destroying or object.leases != 0 or
                (object.references != 0 and !(object.device_lost and object.device_quiesced))) return error.Busy;
            const backing = object.backing orelse return error.Stale;
            if (backing.cookie != ticket.backing.cookie) return error.Stale;
            if (!released) {
                object.phase = .releasing;
                return error.Busy;
            }
            self.committed_bytes -= object.allocation_bytes;
            if (object.references == 0) {
                object.* = .{};
            } else {
                // Retain handle identity for close only, never the freed GPU
                // allocation/cookie or its charged physical capacity.
                object.phase = .live;
                object.backing = null;
                object.owned_cookie = 0;
                object.owned_reference = .{};
                object.allocation_bytes = 0;
            }
            self.collectClosedBudgets();
        }

        pub fn retainsDriver(self: *const Self, owner: Owner) bool {
            for (self.objectsSlice()) |object| {
                if (object.phase != .empty and object.owned_cookie != 0 and object.producer.eql(owner)) return true;
            }
            for (self.objectsSlice()) |object| if (object.backing) |backing| if (backing.driver) |driver| {
                if (driver.eql(owner)) return true;
            };
            for (self.leasesSlice()) |lease| {
                if (lease.handle.id != 0 and lease.owner.eql(owner) and isDevice(lease.access)) return true;
            }
            return false;
        }

        pub fn stats(self: *const Self) Stats {
            var result = Stats{ .bytes = self.committed_bytes };
            // One pass over objects. Aliases,
            // overlapping ranges and multiple lease kinds never double-charge
            // backing. These are overlapping subsets, not sums of allocation.
            for (self.objectsSlice()) |object| {
                if (object.phase == .empty) continue;
                result.objects += 1;
                result.references += object.references;
                result.leases += object.leases;
                if (!object.producer_open or object.phase == .releasing or object.phase == .destroying) result.retained_bytes += object.allocation_bytes;
                const bytes = object.allocation_bytes;
                const device = object.descriptor.location == .device_local;
                if (device) result.device_bytes += bytes else result.system_bytes += bytes;
                if (object.phase == .allocating) result.allocating_bytes += bytes;
                if (object.phase == .releasing or object.phase == .destroying) result.destroying_bytes += bytes;
                if (object.backing != null) {
                    if (device) result.device_backed_bytes += bytes else result.system_backed_bytes += bytes;
                    if (object.leases != 0) {
                        if (device) result.device_pinned_bytes += bytes else result.system_pinned_bytes += bytes;
                    }
                }
                if (object.scanout_leases != 0) result.scanout_pinned_bytes += bytes;
                if (object.mapping_leases != 0) result.device_mapped_bytes += bytes;
            }
            std.debug.assert(result.system_bytes + result.device_bytes == result.bytes);
            return result;
        }

        fn maybeRelease(_: *Self, object: *Object) void {
            if (object.phase != .live or object.leases != 0) return;
            if (object.device_lost and object.device_quiesced and object.backing == null and object.owned_cookie == 0) {
                if (object.references == 0) object.* = .{};
                return;
            }
            if (object.references == 0 or (object.device_lost and object.device_quiesced)) object.phase = .releasing;
        }
        fn nextHandle(self: *Self, slot: usize) Error!Handle {
            if (self.serial == std.math.maxInt(u64) or slot >= std.math.maxInt(u32)) return error.Exhausted;
            self.serial += 1;
            return .{ .id = @intCast(slot + 1), .generation = self.serial };
        }
        fn findObject(self: *Self, handle: Handle) Error!*Object {
            if (handle.id == 0 or handle.id > self.objects.len or handle.generation == 0) return error.Stale;
            const item = &self.objects[handle.id - 1];
            if (item.phase == .empty or !item.handle.eql(handle)) return error.Stale;
            return item;
        }
        fn findReference(self: *Self, handle: Handle, owner: Owner) Error!*Reference {
            if (handle.id == 0 or handle.id > self.references.len or handle.generation == 0) return error.Stale;
            const item = &self.references[handle.id - 1];
            if (!item.handle.eql(handle)) return error.Stale;
            if (!item.owner.eql(owner)) return error.WrongOwner;
            return item;
        }
        fn referencedObject(self: *Self, handle: Handle, owner: Owner) Error!*Object {
            return self.findObject((try self.findReference(handle, owner)).buffer);
        }
        fn findLease(self: *Self, handle: Handle, owner: Owner) Error!*Lease {
            if (handle.id == 0 or handle.id > self.leases.len or handle.generation == 0) return error.Stale;
            const item = &self.leases[handle.id - 1];
            if (!item.handle.eql(handle)) return error.Stale;
            if (!item.owner.eql(owner)) return error.WrongOwner;
            return item;
        }
        fn freeObject(self: *const Self) ?usize {
            for (self.objectsSlice(), 0..) |item, index| if (item.phase == .empty) {
                return index;
            };
            return null;
        }
        fn freeReference(self: *const Self) ?usize {
            for (self.referencesSlice(), 0..) |item, index| if (item.handle.id == 0) {
                return index;
            };
            return null;
        }
        fn freeLease(self: *const Self) ?usize {
            for (self.leasesSlice(), 0..) |item, index| if (item.handle.id == 0) {
                return index;
            };
            return null;
        }
    };
}

pub fn isCpu(access: Access) bool {
    return access == .cpu_read or access == .cpu_write or persistentCpu(access);
}
fn isDevice(access: Access) bool {
    return !isCpu(access);
}
fn persistentCpu(access: Access) bool {
    return access == .cpu_persistent_read or access == .cpu_persistent_write;
}
fn execution(access: Access) bool {
    return access == .device_read or access == .device_write or queued(access);
}
fn conflicts(first: Access, second: Access) bool {
    if (first == .device_mapping or second == .device_mapping) return false;
    // Address lifetime and explicit CPU/GPU synchronization are independent.
    // Ordinary CPU maps and scanout remain exclusive against writers.
    if ((persistentCpu(first) and execution(second)) or (persistentCpu(second) and execution(first))) return false;
    if (queued(first) and queued(second)) return false;
    return writes(first) or writes(second);
}
fn queued(access: Access) bool {
    return access == .queue_read or access == .queue_write;
}
fn writes(access: Access) bool {
    return access == .cpu_write or access == .cpu_persistent_write or access == .device_write or access == .queue_write;
}

test "producer exit preserves imported pixels and outstanding scanout until explicit release" {
    const t = std.testing;
    var store = Table(4, 12, 12){ .budget_bytes = 8192, .producer_budget_bytes = 8192 };
    const baseline = store.stats();
    const producer = Owner{ .kind = .program, .id = 1, .generation = 1 };
    const consumer = Owner{ .kind = .program, .id = 2, .generation = 3 };
    const driver = Owner{ .kind = .driver, .id = 9, .generation = 2 };
    var descriptor = layout.Descriptor{ .bytes = 4096, .width = 32, .height = 32, .format = .xrgb8888, .plane_count = 1, .usage = layout.Usage.cpu_read | layout.Usage.cpu_write | layout.Usage.scanout };
    descriptor.planes[0].pitch = 128;
    const created = try store.begin(producer, descriptor);
    const pixels = try t.allocator.alignedAlloc(u8, .fromByteUnits(4096), created.bytes);
    defer t.allocator.free(pixels);
    @memset(pixels, 0x5A);
    try store.publish(created, .{ .cookie = 51, .cpu_address = @intFromPtr(pixels.ptr), .bytes = pixels.len, .cache = .write_back });
    const imported = try store.import(created.buffer, consumer);
    const hardware = try store.import(created.buffer, driver);
    const first_map = try store.use(imported, consumer, .cpu_read, 0, 4096);
    const second_map = try store.use(imported, consumer, .cpu_read, 64, 64);
    try t.expectError(error.Busy, store.use(created.reference, producer, .cpu_write, 0, 4096));
    const scanout = try store.use(hardware, driver, .scanout, 0, 4096);
    // A capture reader, physical scanout and a new allocation compete while
    // the producer exits. Failed admission must not change any accounting.
    const pressure_owner = Owner{ .kind = .program, .id = 3, .generation = 1 };
    const pressure = try store.begin(pressure_owner, .{ .bytes = 4096 });
    const full = store.stats();
    try t.expectEqual(@as(u64, 4096), full.system_pinned_bytes);
    try t.expectEqual(@as(u64, 4096), full.scanout_pinned_bytes);
    try t.expectEqual(@as(u64, 4096), full.allocating_bytes);
    try t.expectError(error.Budget, store.begin(pressure_owner, .{ .bytes = 1 }));
    try t.expectEqualDeep(full, store.stats());
    store.stoppedOwner(producer);
    try t.expectError(error.Closed, store.import(created.buffer, producer));
    try t.expectError(error.Stale, store.describe(created.reference, producer));
    try t.expectEqual(@as(u8, 0x5A), @as([*]const u8, @ptrFromInt(first_map.backing.cpu_address))[4095]);
    // Capture death retires both CPU mappings; it is not a scanout ACK.
    store.stoppedOwner(consumer);
    try t.expectError(error.Stale, store.endUse(first_map.lease, consumer, false));
    try t.expectError(error.Stale, store.endUse(second_map.lease, consumer, false));
    try t.expectEqual(@as(usize, 1), store.stats().leases);
    try t.expectEqual(@as(u64, 4096), store.stats().scanout_pinned_bytes);
    store.stoppedOwner(driver);
    try t.expect(store.retainsDriver(driver));
    try t.expectEqual(@as(?Release, null), store.pendingRelease());
    try t.expectError(error.Busy, store.endUse(scanout.lease, driver, false));
    try store.endUse(scanout.lease, driver, true);
    const release = store.pendingRelease().?;
    try t.expectEqual(@as(?Release, null), store.pendingRelease());
    try t.expectError(error.Busy, store.finishRelease(release, false));
    try t.expectEqual(@as(u64, 8192), store.stats().bytes);
    const retry = store.pendingRelease().?;
    try t.expectError(error.Stale, store.finishRelease(release, true));
    try store.finishRelease(retry, true);
    try t.expectEqual(@as(u64, 4096), store.stats().bytes);
    try store.abort(pressure);
    try t.expectEqualDeep(baseline, store.stats());
    try t.expectError(error.Stale, store.endUse(scanout.lease, driver, true));
}

test "failed allocation, stopped creation and stale release cannot leak budgets or reuse a live object" {
    const t = std.testing;
    var store = Table(2, 4, 4){ .budget_bytes = 8192, .producer_budget_bytes = 4096 };
    const producer = Owner{ .kind = .program, .id = 1, .generation = 1 };
    const first = try store.begin(producer, .{ .bytes = 1024 });
    try t.expectError(error.Budget, store.begin(producer, .{ .bytes = 1024 }));
    try store.abort(first); // allocator OOM: no backing was published
    const second = try store.begin(producer, .{ .bytes = 1024 });
    try t.expect(!second.buffer.eql(first.buffer));
    store.stoppedOwner(producer);
    try store.publish(second, .{ .cookie = 11, .bytes = second.bytes });
    const release = store.pendingRelease().?;
    try store.finishRelease(release, true);
    const third = try store.begin(.{ .kind = .program, .id = 1, .generation = 2 }, .{ .bytes = 1024 });
    try t.expectError(error.Stale, store.abort(first));
    try t.expectError(error.Stale, store.finishRelease(release, true));
    try store.abort(third);
    try t.expectEqual(@as(usize, 0), store.stats().objects);
}

test "immutable raster exports pin contents after CPU maps end and shares preserve immutability" {
    try persistentCpuLifetime();
    const t = std.testing;
    var store = Table(2, 8, 8){ .budget_bytes = 8192, .producer_budget_bytes = 8192 };
    const producer = Owner{ .kind = .program, .id = 1, .generation = 1 };
    const reader = Owner{ .kind = .program, .id = 2, .generation = 1 };
    const third = Owner{ .kind = .driver, .id = 3, .generation = 2 };
    const created = try store.begin(producer, .{ .bytes = 4096 });
    try store.publish(created, .{ .cookie = 1, .bytes = 4096, .cpu_address = 0x1000, .cache = .write_back });
    const exported = try store.importMode(created.buffer, reader, true);
    const imported = try store.share(exported, third);
    try t.expect(try store.readOnly(imported, third));
    try store.drop(exported, reader);
    try t.expectError(error.Busy, store.use(created.reference, producer, .cpu_write, 0, 4096));
    try t.expectError(error.Unsupported, store.use(imported, third, .cpu_write, 0, 4096));
    try store.drop(imported, third);
    const write = try store.use(created.reference, producer, .cpu_write, 0, 4096);
    try store.endUse(write.lease, producer, false);
    try store.drop(created.reference, producer);
    try store.finishRelease(store.pendingRelease().?, true);
    try t.expectEqual(@as(u64, 0), store.stats().bytes);
}

fn persistentCpuLifetime() !void {
    const t = std.testing;
    var store = Table(2, 8, 8){ .budget_bytes = 8192, .producer_budget_bytes = 8192 };
    const app: Owner = .{ .kind = .program, .id = 1, .generation = 1 };
    const driver: Owner = .{ .kind = .driver, .id = 2, .generation = 7 };
    const queue: Owner = .{ .kind = .kernel, .id = 3, .generation = 1 };
    const created = try store.begin(app, .{ .bytes = 4096, .usage = 15 });
    try store.publish(created, .{ .cookie = 1, .bytes = 4096, .cpu_address = 0x1000, .cache = .write_back });
    const ref = try store.share(created.reference, driver);
    const cpu = try store.use(created.reference, app, .cpu_persistent_write, 0, 4096);
    try t.expectError(error.Busy, store.importMode(created.buffer, driver, true));
    try t.expectError(error.Busy, store.use(created.reference, app, .cpu_read, 0, 4096));
    const gpu = try store.use(ref, driver, .device_write, 0, 4096);
    try t.expectError(error.Busy, store.endUse(gpu.lease, driver, false));
    try store.endUse(gpu.lease, driver, true);
    const queued_use = try store.reserveQueued(created.reference, app, queue, true, 0, 4096);
    try store.endUse(cpu.lease, app, false);
    // Admission works in either order. Neither map removal nor app death is
    // evidence that the independently held device/queue use has completed.
    const next = try store.use(created.reference, app, .cpu_persistent_read, 0, 4096);
    store.stoppedOwner(app);
    try t.expectError(error.Stale, store.useInfo(next.lease, app));
    try store.drop(ref, driver);
    try t.expect(store.pendingRelease() == null);
    try t.expectError(error.Busy, store.endUse(queued_use.lease, queue, false));
    try store.endUse(queued_use.lease, queue, true);
    try store.finishRelease(store.pendingRelease().?, true);
    try t.expectEqual(@as(u64, 0), store.stats().bytes);

    for ([_]@FieldType(Backing, "cache"){ .uncached, .write_combining }) |cache| {
        var other = Table(1, 2, 2){ .budget_bytes = 4096, .producer_budget_bytes = 4096 };
        const item = try other.begin(app, .{ .bytes = 4096, .usage = 15 });
        try other.publish(item, .{ .cookie = 2, .bytes = 4096, .cpu_address = 0x2000, .cache = cache });
        try t.expectError(error.Unsupported, other.use(item.reference, app, .cpu_persistent_write, 0, 4096));
        try other.drop(item.reference, app);
        try other.finishRelease(other.pendingRelease().?, true);
    }
}
