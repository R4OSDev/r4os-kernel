// Buffer lifetime and admission, independent of allocation or GPU registers.
// The enclosing owner serializes calls. Allocation, cache synchronization,
// callbacks and actual release happen outside its no-sleep metadata lock.
const std = @import("std");
pub const layout = @import("gfx_buffer_layout.zig");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_gfx_owner_valid(*const Owner) bool;
extern fn r4os_rust_gfx_owner_equal(*const Owner, *const Owner) bool;
extern fn r4os_rust_gfx_handle_equal(*const Handle, *const Handle) bool;
extern fn r4os_rust_gfx_access_predicate(u32, u8) bool;
extern fn r4os_rust_gfx_access_conflicts(u32, u32) bool;

pub const Owner = extern struct {
    kind: enum(u32) { kernel, program, driver },
    id: u64,
    generation: u64,
    pub fn valid(self: Owner) bool {
        return r4os_rust_gfx_owner_valid(&self);
    }
    pub fn eql(a: Owner, b: Owner) bool {
        return r4os_rust_gfx_owner_equal(&a, &b);
    }
};
pub const Handle = extern struct {
    id: u32 = 0,
    generation: u64 = 0,
    pub fn eql(a: Handle, b: Handle) bool {
        return r4os_rust_gfx_handle_equal(&a, &b);
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
pub const Access = enum(u32) { cpu_read, cpu_write, device_read, device_write, scanout, device_mapping, queue_read, queue_write, cpu_persistent_read, cpu_persistent_write };
pub const Error = layout.Error || error{ Exhausted, Budget, Capacity, Stale, Busy, Closed, WrongOwner };
pub const Create = extern struct { buffer: Handle, reference: Handle, bytes: u64 };
pub const Use = struct { lease: Handle, buffer: Handle, access: Access, backing: Backing, range: layout.Range };
pub const Release = struct { buffer: Handle, backing: Backing, attempt: u64 };
pub const OwnedCreate = extern struct { create: Create, cookie: u64, driver: Owner, binding: layout.Binding };
pub const OwnedRelease = struct { release: Release, driver: Owner, binding: layout.Binding };
pub const BudgetSnapshot = extern struct { limit: u64, charged: u64, closing: bool };
pub const Stats = extern struct {
    objects: usize = 0,
    references: usize = 0,
    leases: usize = 0,
    bytes: u64 = 0,
    retained_bytes: u64 = 0,
    system_bytes: u64 = 0,
    device_bytes: u64 = 0,
    system_backed_bytes: u64 = 0,
    device_backed_bytes: u64 = 0,
    system_pinned_bytes: u64 = 0,
    device_pinned_bytes: u64 = 0,
    scanout_pinned_bytes: u64 = 0,
    device_mapped_bytes: u64 = 0,
    allocating_bytes: u64 = 0,
    destroying_bytes: u64 = 0,
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
    var descriptor: layout.Descriptor = .{ .bytes = 4096, .usage = 63, .format = .argb8888, .width = 32, .height = 32, .plane_count = 1 };
    descriptor.planes[0].pitch = 128;
    // These counts cross the old production pools; none is an implementation
    // limit. Storage replacement must preserve IDs, generations and leases.
    for (0..513) |i| {
        inline for (std.enums.values(Pool)) |kind| {
            const needed: usize = switch (kind) {
                .objects => 1,
                .references => 4,
                .leases => 6,
            };
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
    var large = descriptor;
    large.bytes = 8192;
    const second = try store.beginOwned(gpu, large, 22);
    const other_cpu = try store.begin(other, .{ .bytes = 4096 });
    try t.expect(store.totalBudgetBytes() == 20480 and store.stats().bytes == 20480);
    try t.expect((try store.deviceBudget(gpu, binding)).charged == 12288);
    try t.expect(store.sharedChargedBytes() == 8192);
    try t.expectEqualDeep(try store.deviceBudget(gpu, binding), try store.queryDeviceBudget(binding.adapter, binding.device_generation));
    try t.expectError(error.Stale, store.queryDeviceBudget(binding.adapter, binding.device_generation + 1));
    const replacement_driver: Owner = .{ .kind = .driver, .id = 5, .generation = 8 };
    var replacement_binding = binding;
    replacement_binding.driver_owner = 5;
    try t.expectError(error.Busy, store.setDeviceBudget(replacement_driver, replacement_binding, 8192));
    try t.expectError(error.Budget, store.begin(app, .{ .bytes = 4096 }));
    try store.setDeviceBudget(gpu, binding, 4096);
    try t.expect((try store.deviceBudget(gpu, binding)).charged == 12288 and store.stats().device_bytes == 12288);
    try t.expectError(error.Budget, store.beginOwned(gpu, descriptor, 23));
    var wrong = binding;
    wrong.device_generation += 1;
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
    try store.abort(cpu);
    try store.abort(other_cpu);
    try t.expectEqualDeep(Stats{}, store.stats());
}

fn accountingLifetime() !void {
    const t = std.testing;
    var store = Table(4, 12, 12){ .budget_bytes = 16384, .producer_budget_bytes = 16384 };
    const app: Owner = .{ .kind = .program, .id = 3, .generation = 1 };
    const gpu: Owner = .{ .kind = .driver, .id = 4, .generation = 7 };
    const binding: layout.Binding = .{ .adapter = 2, .driver_owner = 4, .device_generation = 9 };
    var raster: layout.Descriptor = .{ .bytes = 1024, .format = .xrgb8888, .width = 16, .height = 16, .plane_count = 1, .usage = layout.Usage.cpu_read | layout.Usage.scanout };
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
        desc.modifier = 0x0300000000606010;
        desc.width = 5;
        desc.height = 3;
        desc.format = .nv12;
        desc.plane_count = 2;
        desc.planes[0] = .{ .pitch = 64 };
        desc.planes[1] = .{ .offset = 2048, .pitch = 64 };
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
    forged = reserved;
    forged.cookie += 1;
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
    var wrong_binding = binding;
    wrong_binding.device_generation += 1;
    try t.expectEqual(@as(?OwnedRelease, null), try store.takeOwnedRelease(driver, wrong_binding));
    try t.expectEqual(@as(?OwnedRelease, null), try store.takeOwnedRelease(stale, binding));
    const release = (try store.takeOwnedRelease(driver, binding)).?;
    try t.expectEqual(@as(?OwnedRelease, null), try store.takeOwnedRelease(driver, binding));
    try t.expectError(error.Busy, store.finishOwnedRelease(release, driver, false));
    var bad = release;
    bad.release.backing.bytes += 4096;
    try t.expectError(error.Stale, store.finishOwnedRelease(bad, driver, true));
    bad = release;
    bad.release.attempt += 1;
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
    var next = binding;
    next.device_generation += 1;
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

// Private C records carry no Zig optional/auto-layout across the boundary.
const empty_owner: Owner = .{ .kind = .kernel, .id = 0, .generation = 0 };
const NativeBacking = extern struct {
    cookie: u64 = 0,
    cpu_address: u64 = 0,
    bytes: u64 = 0,
    cache: u32 = 0,
    has_driver: bool = false,
    driver: Owner = empty_owner,
    fn from(source: Backing) NativeBacking {
        return .{ .cookie = source.cookie, .cpu_address = source.cpu_address, .bytes = source.bytes, .cache = @intFromEnum(source.cache), .has_driver = source.driver != null, .driver = source.driver orelse empty_owner };
    }
    fn value(self: NativeBacking) Backing {
        return .{ .cookie = self.cookie, .cpu_address = self.cpu_address, .bytes = self.bytes, .cache = @enumFromInt(self.cache), .driver = if (self.has_driver) self.driver else null };
    }
};
pub const NativeUse = extern struct {
    lease: Handle,
    buffer: Handle,
    access: Access,
    backing: NativeBacking,
    range: layout.Range,
    pub fn from(source: Use) NativeUse {
        return .{ .lease = source.lease, .buffer = source.buffer, .access = source.access, .backing = NativeBacking.from(source.backing), .range = source.range };
    }
    pub fn value(self: NativeUse) Use {
        return .{ .lease = self.lease, .buffer = self.buffer, .access = self.access, .backing = self.backing.value(), .range = self.range };
    }
};
const NativeRelease = extern struct {
    buffer: Handle,
    backing: NativeBacking,
    attempt: u64,
    fn from(ticket: Release) NativeRelease {
        return .{ .buffer = ticket.buffer, .backing = NativeBacking.from(ticket.backing), .attempt = ticket.attempt };
    }
    fn value(self: NativeRelease) Release {
        return .{ .buffer = self.buffer, .backing = self.backing.value(), .attempt = self.attempt };
    }
};
const NativeOwnedRelease = extern struct {
    release: NativeRelease,
    driver: Owner,
    binding: layout.Binding,
    fn from(ticket: OwnedRelease) NativeOwnedRelease {
        return .{ .release = NativeRelease.from(ticket.release), .driver = ticket.driver, .binding = ticket.binding };
    }
    fn value(self: NativeOwnedRelease) OwnedRelease {
        return .{ .release = self.release.value(), .driver = self.driver, .binding = self.binding };
    }
};
const NativeOptionalRelease = extern struct { present: bool, value: NativeRelease };
const NativeOptionalOwnedRelease = extern struct { present: bool, value: NativeOwnedRelease };
const Object = extern struct {
    phase: enum(u32) { empty, allocating, live, releasing, destroying } = .empty,
    handle: Handle = .{},
    producer: Owner = empty_owner,
    producer_open: bool = true,
    descriptor: layout.Descriptor = .{ .bytes = 0 },
    allocation_bytes: u64 = 0,
    has_backing: bool = false,
    backing: NativeBacking = .{},
    references: u32 = 0,
    leases: u32 = 0,
    scanout_leases: u32 = 0,
    mapping_leases: u32 = 0,
    release_attempt: u64 = 0,
    owned_cookie: u64 = 0,
    owned_reference: Handle = .{},
    device_lost: bool = false,
    device_quiesced: bool = false,
    pub fn backingValue(self: Object) ?Backing {
        return if (self.has_backing) self.backing.value() else null;
    }
};
const Reference = extern struct { handle: Handle = .{}, buffer: Handle = .{}, owner: Owner = empty_owner, read_only: bool = false, mapping_only: bool = false };
const Lease = extern struct { handle: Handle = .{}, buffer: Handle = .{}, owner: Owner = empty_owner, access: Access = .cpu_read, range: layout.Range = .{ .offset = 0, .bytes = 0 } };
const DeviceBudget = extern struct { present: bool = false, driver: Owner = empty_owner, binding: layout.Binding = .{}, bytes: u64 = 0, closing: bool = false };
pub const NativeView = extern struct {
    objects: [*]Object,
    objects_len: usize,
    references: [*]Reference,
    references_len: usize,
    leases: [*]Lease,
    leases_len: usize,
    serial: *u64,
    committed_bytes: *u64,
    budget_bytes: u64,
    producer_budget_bytes: u64,
    device_budgets: [*]DeviceBudget,
    device_budgets_len: usize,
    dynamic_mask: u32,
};
pub const NativeArgs = extern struct {
    owner: Owner = empty_owner,
    other: Owner = empty_owner,
    handle: Handle = .{},
    binding: layout.Binding = .{},
    offset: u64 = 0,
    bytes: u64 = 0,
    value: u64 = 0,
    flag: bool = false,
    access: u32 = 0,
    pool: u32 = 0,
    input: ?*const anyopaque = null,
    other_input: ?*const anyopaque = null,
    buffer: ?*anyopaque = null,
    len: usize = 0,
};
extern fn r4os_rust_gfx_bo_run(*const NativeView, u32, *const NativeArgs, ?*anyopaque) i32;
fn nativeResult(result: i32) Error!void {
    return switch (result) {
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
        else => unreachable,
    };
}
pub const Pool = enum(u32) { objects, references, leases };
pub fn DynamicTable() type {
    return StorageTable(null, null, null);
}
pub fn Table(comptime objects: usize, comptime references: usize, comptime leases: usize) type {
    return StorageTable(objects, references, leases);
}
fn Slice(comptime SelfPointer: type, comptime T: type) type {
    return if (@typeInfo(SelfPointer).pointer.is_const) []const T else []T;
}
fn StorageTable(comptime object_capacity: ?usize, comptime reference_capacity: ?usize, comptime lease_capacity: ?usize) type {
    comptime {
        for ([_]?usize{ object_capacity, reference_capacity, lease_capacity }) |capacity|
            if (capacity != null and capacity.? > std.math.maxInt(u32)) @compileError("buffer handle capacity exceeds u32");
    }
    return struct {
        const Self = @This();
        objects: if (object_capacity) |n| [n]Object else []Object = if (object_capacity != null) @splat(Object{}) else &.{},
        references: if (reference_capacity) |n| [n]Reference else []Reference = if (reference_capacity != null) @splat(Reference{}) else &.{},
        leases: if (lease_capacity) |n| [n]Lease else []Lease = if (lease_capacity != null) @splat(Lease{}) else &.{},
        serial: u64 = 0,
        committed_bytes: u64 = 0,
        budget_bytes: u64,
        producer_budget_bytes: u64,
        device_budgets: [@min(object_capacity orelse 16, 16)]DeviceBudget = @splat(DeviceBudget{}),
        pub fn Element(comptime kind: Pool) type {
            return switch (kind) {
                .objects => Object,
                .references => Reference,
                .leases => Lease,
            };
        }
        pub fn nativeView(self: anytype) NativeView {
            const objects: Slice(@TypeOf(self), Object) = if (object_capacity != null) &self.objects else self.objects;
            const references: Slice(@TypeOf(self), Reference) = if (reference_capacity != null) &self.references else self.references;
            const leases: Slice(@TypeOf(self), Lease) = if (lease_capacity != null) &self.leases else self.leases;
            return .{ .objects = @constCast(objects.ptr), .objects_len = objects.len, .references = @constCast(references.ptr), .references_len = references.len, .leases = @constCast(leases.ptr), .leases_len = leases.len, .serial = @constCast(&self.serial), .committed_bytes = @constCast(&self.committed_bytes), .budget_bytes = self.budget_bytes, .producer_budget_bytes = self.producer_budget_bytes, .device_budgets = @constCast(&self.device_budgets), .device_budgets_len = self.device_budgets.len, .dynamic_mask = @as(u32, if (object_capacity == null) 1 else 0) | @as(u32, if (reference_capacity == null) 2 else 0) | @as(u32, if (lease_capacity == null) 4 else 0) };
        }
        fn call(self: anytype, comptime op: u32, args: NativeArgs, out: ?*anyopaque) Error!void {
            const view = self.nativeView();
            return nativeResult(r4os_rust_gfx_bo_run(&view, op, &args, out));
        }
        fn result(self: anytype, comptime T: type, comptime op: u32, args: NativeArgs) Error!T {
            var out: T = undefined;
            try self.call(op, args, &out);
            return out;
        }
        pub fn freeSlots(self: *const Self, comptime kind: Pool, needed: usize) usize {
            return self.result(usize, 0, .{ .pool = @intFromEnum(kind), .len = needed }) catch unreachable;
        }
        /// Allocation and tail initialization precede the metadata owner. Rust
        /// copies the latest prefix; only then publish this slice under the same
        /// uninterrupted owner. No Rust borrow survives this publication.
        pub fn replaceStorage(self: *Self, comptime kind: Pool, prepared: []Element(kind)) Error![]Element(kind) {
            try self.call(1, .{ .pool = @intFromEnum(kind), .buffer = @ptrCast(prepared.ptr), .len = prepared.len }, null);
            const capacity = switch (kind) {
                .objects => object_capacity,
                .references => reference_capacity,
                .leases => lease_capacity,
            };
            if (comptime capacity != null) unreachable else {
                const old = @field(self, @tagName(kind));
                @field(self, @tagName(kind)) = prepared;
                return old;
            }
        }
        pub fn setDeviceBudget(self: *Self, driver: Owner, binding: layout.Binding, bytes: u64) Error!void {
            return self.call(2, .{ .owner = driver, .binding = binding, .bytes = bytes }, null);
        }
        pub fn deviceBudget(self: *const Self, driver: Owner, binding: layout.Binding) Error!BudgetSnapshot {
            return self.result(BudgetSnapshot, 3, .{ .owner = driver, .binding = binding });
        }
        pub fn queryDeviceBudget(self: *const Self, adapter: u32, generation: u64) Error!BudgetSnapshot {
            return self.result(BudgetSnapshot, 4, .{ .access = adapter, .value = generation });
        }
        pub fn sharedChargedBytes(self: *const Self) u64 {
            return self.result(u64, 5, .{}) catch unreachable;
        }
        pub fn totalBudgetBytes(self: *const Self) u64 {
            return self.result(u64, 6, .{}) catch unreachable;
        }
        pub fn begin(self: *Self, producer: Owner, descriptor: layout.Descriptor) Error!Create {
            return self.result(Create, 7, .{ .owner = producer, .input = &descriptor });
        }
        pub fn beginOwned(self: *Self, driver: Owner, descriptor: layout.Descriptor, cookie: u64) Error!OwnedCreate {
            return self.result(OwnedCreate, 8, .{ .owner = driver, .input = &descriptor, .value = cookie });
        }
        pub fn commitOwned(self: *Self, ticket: OwnedCreate, driver: Owner) Error!void {
            return self.call(9, .{ .owner = driver, .input = &ticket }, null);
        }
        pub fn abortOwned(self: *Self, ticket: OwnedCreate, driver: Owner, quiesced: bool) Error!void {
            return self.call(10, .{ .owner = driver, .input = &ticket, .flag = quiesced }, null);
        }
        pub fn publish(self: *Self, ticket: Create, backing: Backing) Error!void {
            const raw = NativeBacking.from(backing);
            return self.call(11, .{ .input = &ticket, .other_input = &raw }, null);
        }
        pub fn abort(self: *Self, ticket: Create) Error!void {
            return self.call(12, .{ .input = &ticket }, null);
        }
        pub fn import(self: *Self, buffer: Handle, consumer: Owner) Error!Handle {
            return self.result(Handle, 13, .{ .handle = buffer, .owner = consumer });
        }
        pub fn importMode(self: *Self, buffer: Handle, consumer: Owner, read_only: bool) Error!Handle {
            return self.result(Handle, 14, .{ .handle = buffer, .owner = consumer, .flag = read_only });
        }
        pub fn describe(self: *Self, reference: Handle, owner: Owner) Error!layout.Descriptor {
            return self.result(layout.Descriptor, 15, .{ .handle = reference, .owner = owner });
        }
        pub fn share(self: *Self, handle: Handle, consumer: Owner) Error!Handle {
            return self.result(Handle, 16, .{ .handle = handle, .owner = consumer });
        }
        pub fn readOnly(self: *Self, handle: Handle, owner: Owner) Error!bool {
            return self.result(bool, 17, .{ .handle = handle, .owner = owner });
        }
        pub fn mappingOnly(self: *Self, handle: Handle, owner: Owner) Error!bool {
            return self.result(bool, 18, .{ .handle = handle, .owner = owner });
        }
        pub fn drop(self: *Self, handle: Handle, owner: Owner) Error!void {
            return self.call(19, .{ .handle = handle, .owner = owner }, null);
        }
        pub fn use(self: *Self, reference: Handle, owner: Owner, access: Access, offset: u64, bytes: u64) Error!Use {
            return (try self.result(NativeUse, 20, .{ .handle = reference, .owner = owner, .access = @intFromEnum(access), .offset = offset, .bytes = bytes })).value();
        }
        pub fn reserveQueued(self: *Self, reference: Handle, producer: Owner, queue_owner: Owner, write: bool, offset: u64, bytes: u64) Error!Use {
            return (try self.result(NativeUse, 21, .{ .handle = reference, .owner = producer, .other = queue_owner, .flag = write, .offset = offset, .bytes = bytes })).value();
        }
        pub fn retainQueued(self: *Self, handle: Handle, queue_owner: Owner, driver: Owner) Error!Handle {
            return self.result(Handle, 22, .{ .handle = handle, .owner = queue_owner, .other = driver });
        }
        pub fn retainScanout(self: *Self, handle: Handle, queue_owner: Owner, driver: Owner) Error!Handle {
            return self.result(Handle, 23, .{ .handle = handle, .owner = queue_owner, .other = driver });
        }
        pub fn endUse(self: *Self, handle: Handle, owner: Owner, device_quiesced: bool) Error!void {
            return self.call(24, .{ .handle = handle, .owner = owner, .flag = device_quiesced }, null);
        }
        pub fn useInfo(self: *Self, handle: Handle, owner: Owner) Error!Use {
            return (try self.result(NativeUse, 25, .{ .handle = handle, .owner = owner })).value();
        }
        pub fn bufferFor(self: *Self, handle: Handle, owner: Owner) Error!Handle {
            return self.result(Handle, 26, .{ .handle = handle, .owner = owner });
        }
        pub fn loseDevice(self: *Self, driver: Owner, binding: layout.Binding, quiesced: bool) Error!void {
            return self.call(27, .{ .owner = driver, .binding = binding, .flag = quiesced }, null);
        }
        pub fn stoppedOwner(self: *Self, owner: Owner) void {
            self.call(28, .{ .owner = owner }, null) catch unreachable;
        }
        pub fn pendingReleaseForOwner(self: *const Self, producer: Owner) bool {
            return self.result(bool, 29, .{ .owner = producer }) catch unreachable;
        }
        pub fn pendingRelease(self: *Self) ?Release {
            const raw = self.result(NativeOptionalRelease, 30, .{}) catch unreachable;
            return if (raw.present) raw.value.value() else null;
        }
        pub fn pendingSystemRelease(self: *Self) ?Release {
            const raw = self.result(NativeOptionalRelease, 31, .{}) catch unreachable;
            return if (raw.present) raw.value.value() else null;
        }
        pub fn takeOwnedRelease(self: *Self, driver: Owner, binding: layout.Binding) Error!?OwnedRelease {
            const raw = try self.result(NativeOptionalOwnedRelease, 32, .{ .owner = driver, .binding = binding });
            return if (raw.present) raw.value.value() else null;
        }
        pub fn finishOwnedRelease(self: *Self, ticket: OwnedRelease, driver: Owner, quiesced: bool) Error!void {
            const raw = NativeOwnedRelease.from(ticket);
            return self.call(33, .{ .input = &raw, .owner = driver, .flag = quiesced }, null);
        }
        pub fn finishRelease(self: *Self, ticket: Release, released: bool) Error!void {
            const raw = NativeRelease.from(ticket);
            return self.call(34, .{ .input = &raw, .flag = released }, null);
        }
        pub fn retainsDriver(self: *const Self, owner: Owner) bool {
            return self.result(bool, 35, .{ .owner = owner }) catch unreachable;
        }
        pub fn stats(self: *const Self) Stats {
            return self.result(Stats, 36, .{}) catch unreachable;
        }
    };
}

comptime {
    if (@sizeOf(Owner) != 24 or @alignOf(Owner) != 8) @compileError("native BO Owner layout");
    if (@offsetOf(Owner, "kind") != 0) @compileError("native BO Owner.kind");
    if (@offsetOf(Owner, "id") != 8) @compileError("native BO Owner.id");
    if (@offsetOf(Owner, "generation") != 16) @compileError("native BO Owner.generation");
    if (@sizeOf(Handle) != 16 or @alignOf(Handle) != 8) @compileError("native BO Handle layout");
    if (@offsetOf(Handle, "id") != 0) @compileError("native BO Handle.id");
    if (@offsetOf(Handle, "generation") != 8) @compileError("native BO Handle.generation");
    if (@sizeOf(layout.Binding) != 16 or @alignOf(layout.Binding) != 8) @compileError("native BO Binding layout");
    if (@offsetOf(layout.Binding, "adapter") != 0) @compileError("native BO Binding.adapter");
    if (@offsetOf(layout.Binding, "driver_owner") != 4) @compileError("native BO Binding.driver_owner");
    if (@offsetOf(layout.Binding, "device_generation") != 8) @compileError("native BO Binding.device_generation");
    if (@sizeOf(layout.Plane) != 16 or @alignOf(layout.Plane) != 8) @compileError("native BO Plane layout");
    if (@offsetOf(layout.Plane, "offset") != 0) @compileError("native BO Plane.offset");
    if (@offsetOf(layout.Plane, "pitch") != 8) @compileError("native BO Plane.pitch");
    if (@sizeOf(layout.Range) != 16 or @alignOf(layout.Range) != 8) @compileError("native BO Range layout");
    if (@offsetOf(layout.Range, "offset") != 0) @compileError("native BO Range.offset");
    if (@offsetOf(layout.Range, "bytes") != 8) @compileError("native BO Range.bytes");
    if (@sizeOf(layout.Descriptor) != 128 or @alignOf(layout.Descriptor) != 8) @compileError("native BO Descriptor layout");
    if (@offsetOf(layout.Descriptor, "bytes") != 0) @compileError("native BO Descriptor.bytes");
    if (@offsetOf(layout.Descriptor, "alignment") != 8) @compileError("native BO Descriptor.alignment");
    if (@offsetOf(layout.Descriptor, "modifier") != 16) @compileError("native BO Descriptor.modifier");
    if (@offsetOf(layout.Descriptor, "width") != 24) @compileError("native BO Descriptor.width");
    if (@offsetOf(layout.Descriptor, "height") != 28) @compileError("native BO Descriptor.height");
    if (@offsetOf(layout.Descriptor, "format") != 32) @compileError("native BO Descriptor.format");
    if (@offsetOf(layout.Descriptor, "plane_count") != 36) @compileError("native BO Descriptor.plane_count");
    if (@offsetOf(layout.Descriptor, "planes") != 40) @compileError("native BO Descriptor.planes");
    if (@offsetOf(layout.Descriptor, "usage") != 104) @compileError("native BO Descriptor.usage");
    if (@offsetOf(layout.Descriptor, "location") != 108) @compileError("native BO Descriptor.location");
    if (@offsetOf(layout.Descriptor, "binding") != 112) @compileError("native BO Descriptor.binding");
    if (@sizeOf(NativeBacking) != 56 or @alignOf(NativeBacking) != 8) @compileError("native BO Backing layout");
    if (@offsetOf(NativeBacking, "cookie") != 0) @compileError("native BO Backing.cookie");
    if (@offsetOf(NativeBacking, "cpu_address") != 8) @compileError("native BO Backing.cpu_address");
    if (@offsetOf(NativeBacking, "bytes") != 16) @compileError("native BO Backing.bytes");
    if (@offsetOf(NativeBacking, "cache") != 24) @compileError("native BO Backing.cache");
    if (@offsetOf(NativeBacking, "has_driver") != 28) @compileError("native BO Backing.has_driver");
    if (@offsetOf(NativeBacking, "driver") != 32) @compileError("native BO Backing.driver");
    if (@sizeOf(Create) != 40 or @alignOf(Create) != 8) @compileError("native BO Create layout");
    if (@offsetOf(Create, "buffer") != 0) @compileError("native BO Create.buffer");
    if (@offsetOf(Create, "reference") != 16) @compileError("native BO Create.reference");
    if (@offsetOf(Create, "bytes") != 32) @compileError("native BO Create.bytes");
    if (@sizeOf(NativeUse) != 112 or @alignOf(NativeUse) != 8) @compileError("native BO Use layout");
    if (@offsetOf(NativeUse, "lease") != 0) @compileError("native BO Use.lease");
    if (@offsetOf(NativeUse, "buffer") != 16) @compileError("native BO Use.buffer");
    if (@offsetOf(NativeUse, "access") != 32) @compileError("native BO Use.access");
    if (@offsetOf(NativeUse, "backing") != 40) @compileError("native BO Use.backing");
    if (@offsetOf(NativeUse, "range") != 96) @compileError("native BO Use.range");
    if (@sizeOf(NativeRelease) != 80 or @alignOf(NativeRelease) != 8) @compileError("native BO Release layout");
    if (@offsetOf(NativeRelease, "buffer") != 0) @compileError("native BO Release.buffer");
    if (@offsetOf(NativeRelease, "backing") != 16) @compileError("native BO Release.backing");
    if (@offsetOf(NativeRelease, "attempt") != 72) @compileError("native BO Release.attempt");
    if (@sizeOf(OwnedCreate) != 88 or @alignOf(OwnedCreate) != 8) @compileError("native BO OwnedCreate layout");
    if (@offsetOf(OwnedCreate, "create") != 0) @compileError("native BO OwnedCreate.create");
    if (@offsetOf(OwnedCreate, "cookie") != 40) @compileError("native BO OwnedCreate.cookie");
    if (@offsetOf(OwnedCreate, "driver") != 48) @compileError("native BO OwnedCreate.driver");
    if (@offsetOf(OwnedCreate, "binding") != 72) @compileError("native BO OwnedCreate.binding");
    if (@sizeOf(NativeOwnedRelease) != 120 or @alignOf(NativeOwnedRelease) != 8) @compileError("native BO OwnedRelease layout");
    if (@offsetOf(NativeOwnedRelease, "release") != 0) @compileError("native BO OwnedRelease.release");
    if (@offsetOf(NativeOwnedRelease, "driver") != 80) @compileError("native BO OwnedRelease.driver");
    if (@offsetOf(NativeOwnedRelease, "binding") != 104) @compileError("native BO OwnedRelease.binding");
    if (@sizeOf(NativeOptionalRelease) != 88 or @alignOf(NativeOptionalRelease) != 8) @compileError("native BO OptionalRelease layout");
    if (@offsetOf(NativeOptionalRelease, "present") != 0) @compileError("native BO OptionalRelease.present");
    if (@offsetOf(NativeOptionalRelease, "value") != 8) @compileError("native BO OptionalRelease.value");
    if (@sizeOf(NativeOptionalOwnedRelease) != 128 or @alignOf(NativeOptionalOwnedRelease) != 8) @compileError("native BO OptionalOwnedRelease layout");
    if (@offsetOf(NativeOptionalOwnedRelease, "present") != 0) @compileError("native BO OptionalOwnedRelease.present");
    if (@offsetOf(NativeOptionalOwnedRelease, "value") != 8) @compileError("native BO OptionalOwnedRelease.value");
    if (@sizeOf(BudgetSnapshot) != 24 or @alignOf(BudgetSnapshot) != 8) @compileError("native BO BudgetSnapshot layout");
    if (@offsetOf(BudgetSnapshot, "limit") != 0) @compileError("native BO BudgetSnapshot.limit");
    if (@offsetOf(BudgetSnapshot, "charged") != 8) @compileError("native BO BudgetSnapshot.charged");
    if (@offsetOf(BudgetSnapshot, "closing") != 16) @compileError("native BO BudgetSnapshot.closing");
    if (@sizeOf(Stats) != 120 or @alignOf(Stats) != 8) @compileError("native BO Stats layout");
    if (@offsetOf(Stats, "objects") != 0) @compileError("native BO Stats.objects");
    if (@offsetOf(Stats, "references") != 8) @compileError("native BO Stats.references");
    if (@offsetOf(Stats, "leases") != 16) @compileError("native BO Stats.leases");
    if (@offsetOf(Stats, "bytes") != 24) @compileError("native BO Stats.bytes");
    if (@offsetOf(Stats, "retained_bytes") != 32) @compileError("native BO Stats.retained_bytes");
    if (@offsetOf(Stats, "system_bytes") != 40) @compileError("native BO Stats.system_bytes");
    if (@offsetOf(Stats, "device_bytes") != 48) @compileError("native BO Stats.device_bytes");
    if (@offsetOf(Stats, "system_backed_bytes") != 56) @compileError("native BO Stats.system_backed_bytes");
    if (@offsetOf(Stats, "device_backed_bytes") != 64) @compileError("native BO Stats.device_backed_bytes");
    if (@offsetOf(Stats, "system_pinned_bytes") != 72) @compileError("native BO Stats.system_pinned_bytes");
    if (@offsetOf(Stats, "device_pinned_bytes") != 80) @compileError("native BO Stats.device_pinned_bytes");
    if (@offsetOf(Stats, "scanout_pinned_bytes") != 88) @compileError("native BO Stats.scanout_pinned_bytes");
    if (@offsetOf(Stats, "device_mapped_bytes") != 96) @compileError("native BO Stats.device_mapped_bytes");
    if (@offsetOf(Stats, "allocating_bytes") != 104) @compileError("native BO Stats.allocating_bytes");
    if (@offsetOf(Stats, "destroying_bytes") != 112) @compileError("native BO Stats.destroying_bytes");
    if (@sizeOf(Object) != 312 or @alignOf(Object) != 8) @compileError("native BO Object layout");
    if (@offsetOf(Object, "phase") != 0) @compileError("native BO Object.phase");
    if (@offsetOf(Object, "handle") != 8) @compileError("native BO Object.handle");
    if (@offsetOf(Object, "producer") != 24) @compileError("native BO Object.producer");
    if (@offsetOf(Object, "producer_open") != 48) @compileError("native BO Object.producer_open");
    if (@offsetOf(Object, "descriptor") != 56) @compileError("native BO Object.descriptor");
    if (@offsetOf(Object, "allocation_bytes") != 184) @compileError("native BO Object.allocation_bytes");
    if (@offsetOf(Object, "has_backing") != 192) @compileError("native BO Object.has_backing");
    if (@offsetOf(Object, "backing") != 200) @compileError("native BO Object.backing");
    if (@offsetOf(Object, "references") != 256) @compileError("native BO Object.references");
    if (@offsetOf(Object, "leases") != 260) @compileError("native BO Object.leases");
    if (@offsetOf(Object, "scanout_leases") != 264) @compileError("native BO Object.scanout_leases");
    if (@offsetOf(Object, "mapping_leases") != 268) @compileError("native BO Object.mapping_leases");
    if (@offsetOf(Object, "release_attempt") != 272) @compileError("native BO Object.release_attempt");
    if (@offsetOf(Object, "owned_cookie") != 280) @compileError("native BO Object.owned_cookie");
    if (@offsetOf(Object, "owned_reference") != 288) @compileError("native BO Object.owned_reference");
    if (@offsetOf(Object, "device_lost") != 304) @compileError("native BO Object.device_lost");
    if (@offsetOf(Object, "device_quiesced") != 305) @compileError("native BO Object.device_quiesced");
    if (@sizeOf(Reference) != 64 or @alignOf(Reference) != 8) @compileError("native BO Reference layout");
    if (@offsetOf(Reference, "handle") != 0) @compileError("native BO Reference.handle");
    if (@offsetOf(Reference, "buffer") != 16) @compileError("native BO Reference.buffer");
    if (@offsetOf(Reference, "owner") != 32) @compileError("native BO Reference.owner");
    if (@offsetOf(Reference, "read_only") != 56) @compileError("native BO Reference.read_only");
    if (@offsetOf(Reference, "mapping_only") != 57) @compileError("native BO Reference.mapping_only");
    if (@sizeOf(Lease) != 80 or @alignOf(Lease) != 8) @compileError("native BO Lease layout");
    if (@offsetOf(Lease, "handle") != 0) @compileError("native BO Lease.handle");
    if (@offsetOf(Lease, "buffer") != 16) @compileError("native BO Lease.buffer");
    if (@offsetOf(Lease, "owner") != 32) @compileError("native BO Lease.owner");
    if (@offsetOf(Lease, "access") != 56) @compileError("native BO Lease.access");
    if (@offsetOf(Lease, "range") != 64) @compileError("native BO Lease.range");
    if (@sizeOf(DeviceBudget) != 64 or @alignOf(DeviceBudget) != 8) @compileError("native BO DeviceBudget layout");
    if (@offsetOf(DeviceBudget, "present") != 0) @compileError("native BO DeviceBudget.present");
    if (@offsetOf(DeviceBudget, "driver") != 8) @compileError("native BO DeviceBudget.driver");
    if (@offsetOf(DeviceBudget, "binding") != 32) @compileError("native BO DeviceBudget.binding");
    if (@offsetOf(DeviceBudget, "bytes") != 48) @compileError("native BO DeviceBudget.bytes");
    if (@offsetOf(DeviceBudget, "closing") != 56) @compileError("native BO DeviceBudget.closing");
    if (@sizeOf(NativeView) != 104 or @alignOf(NativeView) != 8) @compileError("native BO RawView layout");
    if (@offsetOf(NativeView, "objects") != 0) @compileError("native BO RawView.objects");
    if (@offsetOf(NativeView, "objects_len") != 8) @compileError("native BO RawView.objects_len");
    if (@offsetOf(NativeView, "references") != 16) @compileError("native BO RawView.references");
    if (@offsetOf(NativeView, "references_len") != 24) @compileError("native BO RawView.references_len");
    if (@offsetOf(NativeView, "leases") != 32) @compileError("native BO RawView.leases");
    if (@offsetOf(NativeView, "leases_len") != 40) @compileError("native BO RawView.leases_len");
    if (@offsetOf(NativeView, "serial") != 48) @compileError("native BO RawView.serial");
    if (@offsetOf(NativeView, "committed_bytes") != 56) @compileError("native BO RawView.committed_bytes");
    if (@offsetOf(NativeView, "budget_bytes") != 64) @compileError("native BO RawView.budget_bytes");
    if (@offsetOf(NativeView, "producer_budget_bytes") != 72) @compileError("native BO RawView.producer_budget_bytes");
    if (@offsetOf(NativeView, "device_budgets") != 80) @compileError("native BO RawView.device_budgets");
    if (@offsetOf(NativeView, "device_budgets_len") != 88) @compileError("native BO RawView.device_budgets_len");
    if (@offsetOf(NativeView, "dynamic_mask") != 96) @compileError("native BO RawView.dynamic_mask");
    if (@sizeOf(NativeArgs) != 152 or @alignOf(NativeArgs) != 8) @compileError("native BO Args layout");
    if (@offsetOf(NativeArgs, "owner") != 0) @compileError("native BO Args.owner");
    if (@offsetOf(NativeArgs, "other") != 24) @compileError("native BO Args.other");
    if (@offsetOf(NativeArgs, "handle") != 48) @compileError("native BO Args.handle");
    if (@offsetOf(NativeArgs, "binding") != 64) @compileError("native BO Args.binding");
    if (@offsetOf(NativeArgs, "offset") != 80) @compileError("native BO Args.offset");
    if (@offsetOf(NativeArgs, "bytes") != 88) @compileError("native BO Args.bytes");
    if (@offsetOf(NativeArgs, "value") != 96) @compileError("native BO Args.value");
    if (@offsetOf(NativeArgs, "flag") != 104) @compileError("native BO Args.flag");
    if (@offsetOf(NativeArgs, "access") != 108) @compileError("native BO Args.access");
    if (@offsetOf(NativeArgs, "pool") != 112) @compileError("native BO Args.pool");
    if (@offsetOf(NativeArgs, "input") != 120) @compileError("native BO Args.input");
    if (@offsetOf(NativeArgs, "other_input") != 128) @compileError("native BO Args.other_input");
    if (@offsetOf(NativeArgs, "buffer") != 136) @compileError("native BO Args.buffer");
    if (@offsetOf(NativeArgs, "len") != 144) @compileError("native BO Args.len");
}

pub fn isCpu(access: Access) bool {
    return r4os_rust_gfx_access_predicate(@intFromEnum(access), 0);
}
fn isDevice(access: Access) bool {
    return r4os_rust_gfx_access_predicate(@intFromEnum(access), 1);
}
fn persistentCpu(access: Access) bool {
    return r4os_rust_gfx_access_predicate(@intFromEnum(access), 2);
}
fn execution(access: Access) bool {
    return r4os_rust_gfx_access_predicate(@intFromEnum(access), 3);
}
fn conflicts(first: Access, second: Access) bool {
    return r4os_rust_gfx_access_conflicts(@intFromEnum(first), @intFromEnum(second));
}
fn queued(access: Access) bool {
    return r4os_rust_gfx_access_predicate(@intFromEnum(access), 4);
}
fn writes(access: Access) bool {
    return r4os_rust_gfx_access_predicate(@intFromEnum(access), 5);
}

comptime {
    if (@sizeOf(Owner) != 24 or @alignOf(Owner) != 8 or @offsetOf(Owner, "kind") != 0 or @offsetOf(Owner, "id") != 8 or @offsetOf(Owner, "generation") != 16) @compileError("Native BO identity C layout");
    if (@sizeOf(Handle) != 16 or @alignOf(Handle) != 8 or @offsetOf(Handle, "id") != 0 or @offsetOf(Handle, "generation") != 8 or @sizeOf(Access) != 4) @compileError("Native BO handle/access C layout");
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

/// Stateless view of the one native runtime owner. Only immutable admission
/// limits are projected here; no second table, generation or resource is held.
pub fn GlobalTable(comptime budget: u64, comptime producer_budget: u64) type {
    return struct {
        const Self = @This();
        budget_bytes: u64 = budget,
        producer_budget_bytes: u64 = producer_budget,
        fn call(_: *const Self, comptime op: u32, args: NativeArgs, out: ?*anyopaque) Error!void {
            return nativeResult(r4os_rust_gfx_global_bo_run(op, &args, out));
        }
        fn result(self: *const Self, comptime T: type, comptime op: u32, args: NativeArgs) Error!T {
            var out: T = undefined;
            try self.call(op, args, &out);
            return out;
        }
        pub fn setDeviceBudget(self: *const Self, driver: Owner, binding: layout.Binding, bytes: u64) Error!void {
            return self.call(2, .{ .owner = driver, .binding = binding, .bytes = bytes }, null);
        }
        pub fn deviceBudget(self: *const Self, driver: Owner, binding: layout.Binding) Error!BudgetSnapshot {
            return self.result(BudgetSnapshot, 3, .{ .owner = driver, .binding = binding });
        }
        pub fn queryDeviceBudget(self: *const Self, adapter: u32, generation: u64) Error!BudgetSnapshot {
            return self.result(BudgetSnapshot, 4, .{ .access = adapter, .value = generation });
        }
        pub fn sharedChargedBytes(self: *const Self) u64 {
            return self.result(u64, 5, .{}) catch unreachable;
        }
        pub fn totalBudgetBytes(self: *const Self) u64 {
            return self.result(u64, 6, .{}) catch unreachable;
        }
        pub fn begin(self: *const Self, producer: Owner, descriptor: layout.Descriptor) Error!Create {
            return self.result(Create, 7, .{ .owner = producer, .input = &descriptor });
        }
        pub fn beginOwned(self: *const Self, driver: Owner, descriptor: layout.Descriptor, cookie: u64) Error!OwnedCreate {
            return self.result(OwnedCreate, 8, .{ .owner = driver, .input = &descriptor, .value = cookie });
        }
        pub fn commitOwned(self: *const Self, ticket: OwnedCreate, driver: Owner) Error!void {
            return self.call(9, .{ .owner = driver, .input = &ticket }, null);
        }
        pub fn abortOwned(self: *const Self, ticket: OwnedCreate, driver: Owner, quiesced: bool) Error!void {
            return self.call(10, .{ .owner = driver, .input = &ticket, .flag = quiesced }, null);
        }
        pub fn publish(self: *const Self, ticket: Create, backing: Backing) Error!void {
            const raw = NativeBacking.from(backing);
            return self.call(11, .{ .input = &ticket, .other_input = &raw }, null);
        }
        pub fn abort(self: *const Self, ticket: Create) Error!void {
            return self.call(12, .{ .input = &ticket }, null);
        }
        pub fn import(self: *const Self, buffer: Handle, consumer: Owner) Error!Handle {
            return self.result(Handle, 13, .{ .handle = buffer, .owner = consumer });
        }
        pub fn importMode(self: *const Self, buffer: Handle, consumer: Owner, read_only: bool) Error!Handle {
            return self.result(Handle, 14, .{ .handle = buffer, .owner = consumer, .flag = read_only });
        }
        pub fn describe(self: *const Self, reference: Handle, owner: Owner) Error!layout.Descriptor {
            return self.result(layout.Descriptor, 15, .{ .handle = reference, .owner = owner });
        }
        pub fn share(self: *const Self, handle: Handle, consumer: Owner) Error!Handle {
            return self.result(Handle, 16, .{ .handle = handle, .owner = consumer });
        }
        pub fn readOnly(self: *const Self, handle: Handle, owner: Owner) Error!bool {
            return self.result(bool, 17, .{ .handle = handle, .owner = owner });
        }
        pub fn mappingOnly(self: *const Self, handle: Handle, owner: Owner) Error!bool {
            return self.result(bool, 18, .{ .handle = handle, .owner = owner });
        }
        pub fn drop(self: *const Self, handle: Handle, owner: Owner) Error!void {
            return self.call(19, .{ .handle = handle, .owner = owner }, null);
        }
        pub fn use(self: *const Self, reference: Handle, owner: Owner, access: Access, offset: u64, bytes: u64) Error!Use {
            return (try self.result(NativeUse, 20, .{ .handle = reference, .owner = owner, .access = @intFromEnum(access), .offset = offset, .bytes = bytes })).value();
        }
        pub fn reserveQueued(self: *const Self, reference: Handle, producer: Owner, queue_owner: Owner, write: bool, offset: u64, bytes: u64) Error!Use {
            return (try self.result(NativeUse, 21, .{ .handle = reference, .owner = producer, .other = queue_owner, .flag = write, .offset = offset, .bytes = bytes })).value();
        }
        pub fn retainQueued(self: *const Self, handle: Handle, queue_owner: Owner, driver: Owner) Error!Handle {
            return self.result(Handle, 22, .{ .handle = handle, .owner = queue_owner, .other = driver });
        }
        pub fn retainScanout(self: *const Self, handle: Handle, queue_owner: Owner, driver: Owner) Error!Handle {
            return self.result(Handle, 23, .{ .handle = handle, .owner = queue_owner, .other = driver });
        }
        pub fn endUse(self: *const Self, handle: Handle, owner: Owner, device_quiesced: bool) Error!void {
            return self.call(24, .{ .handle = handle, .owner = owner, .flag = device_quiesced }, null);
        }
        pub fn useInfo(self: *const Self, handle: Handle, owner: Owner) Error!Use {
            return (try self.result(NativeUse, 25, .{ .handle = handle, .owner = owner })).value();
        }
        pub fn bufferFor(self: *const Self, handle: Handle, owner: Owner) Error!Handle {
            return self.result(Handle, 26, .{ .handle = handle, .owner = owner });
        }
        pub fn loseDevice(self: *const Self, driver: Owner, binding: layout.Binding, quiesced: bool) Error!void {
            return self.call(27, .{ .owner = driver, .binding = binding, .flag = quiesced }, null);
        }
        pub fn stoppedOwner(self: *const Self, owner: Owner) void {
            self.call(28, .{ .owner = owner }, null) catch unreachable;
        }
        pub fn pendingReleaseForOwner(self: *const Self, producer: Owner) bool {
            return self.result(bool, 29, .{ .owner = producer }) catch unreachable;
        }
        pub fn pendingRelease(self: *const Self) ?Release {
            const raw = self.result(NativeOptionalRelease, 30, .{}) catch unreachable;
            return if (raw.present) raw.value.value() else null;
        }
        pub fn pendingSystemRelease(self: *const Self) ?Release {
            const raw = self.result(NativeOptionalRelease, 31, .{}) catch unreachable;
            return if (raw.present) raw.value.value() else null;
        }
        pub fn takeOwnedRelease(self: *const Self, driver: Owner, binding: layout.Binding) Error!?OwnedRelease {
            const raw = try self.result(NativeOptionalOwnedRelease, 32, .{ .owner = driver, .binding = binding });
            return if (raw.present) raw.value.value() else null;
        }
        pub fn finishOwnedRelease(self: *const Self, ticket: OwnedRelease, driver: Owner, quiesced: bool) Error!void {
            const raw = NativeOwnedRelease.from(ticket);
            return self.call(33, .{ .input = &raw, .owner = driver, .flag = quiesced }, null);
        }
        pub fn finishRelease(self: *const Self, ticket: Release, released: bool) Error!void {
            const raw = NativeRelease.from(ticket);
            return self.call(34, .{ .input = &raw, .flag = released }, null);
        }
        pub fn retainsDriver(self: *const Self, owner: Owner) bool {
            return self.result(bool, 35, .{ .owner = owner }) catch unreachable;
        }
        pub fn stats(self: *const Self) Stats {
            return self.result(Stats, 36, .{}) catch unreachable;
        }
    };
}
extern fn r4os_rust_gfx_global_bo_run(u32, *const NativeArgs, ?*anyopaque) i32;
