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
pub const Access = enum { cpu_read, cpu_write, device_read, device_write, scanout, device_mapping, queue_read, queue_write };
pub const Error = layout.Error || error{ Exhausted, Budget, Capacity, Stale, Busy, Closed, WrongOwner };
pub const Create = struct { buffer: Handle, reference: Handle, bytes: u64 };
pub const Use = struct { lease: Handle, buffer: Handle, access: Access, backing: Backing, range: layout.Range };
pub const Release = struct { buffer: Handle, backing: Backing, attempt: u64 };
pub const Stats = struct { objects: usize = 0, references: usize = 0, leases: usize = 0, bytes: u64 = 0, retained_bytes: u64 = 0 };

test "CPU backing release failure remains visible to the exact driver epoch without public references" {
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

pub fn Table(comptime object_capacity: usize, comptime reference_capacity: usize, comptime lease_capacity: usize) type {
    comptime {
        if (object_capacity > std.math.maxInt(u32) or reference_capacity > std.math.maxInt(u32) or lease_capacity > std.math.maxInt(u32)) @compileError("buffer handle capacity exceeds u32");
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
            release_attempt: u64 = 0,
        };
        const Reference = struct { handle: Handle = .{}, buffer: Handle = .{}, owner: Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, read_only: bool = false, mapping_only: bool = false };
        const Lease = struct { handle: Handle = .{}, buffer: Handle = .{}, owner: Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, access: Access = .cpu_read, range: layout.Range = .{ .offset = 0, .bytes = 0 } };

        objects: [object_capacity]Object = .{Object{}} ** object_capacity,
        references: [reference_capacity]Reference = .{Reference{}} ** reference_capacity,
        leases: [lease_capacity]Lease = .{Lease{}} ** lease_capacity,
        serial: u64 = 0,
        committed_bytes: u64 = 0,
        budget_bytes: u64,
        producer_budget_bytes: u64,

        pub fn begin(self: *Self, producer: Owner, descriptor: layout.Descriptor) Error!Create {
            if (!producer.valid()) return error.Invalid;
            const validated = try layout.validate(descriptor);
            if (self.committed_bytes > self.budget_bytes or validated.allocation_bytes > self.budget_bytes - self.committed_bytes) return error.Budget;
            var owner_bytes: u64 = 0;
            for (&self.objects) |object| {
                if (object.phase != .empty and object.producer.eql(producer)) owner_bytes += object.allocation_bytes;
            }
            if (owner_bytes > self.producer_budget_bytes or validated.allocation_bytes > self.producer_budget_bytes - owner_bytes) return error.Budget;
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
            for (&self.references) |*reference| if (reference.buffer.eql(ticket.buffer)) {
                reference.* = .{};
            };
            self.committed_bytes -= object.allocation_bytes;
            object.* = .{};
        }

        pub fn import(self: *Self, buffer: Handle, consumer: Owner) Error!Handle {
            return self.importMode(buffer, consumer, false);
        }

        pub fn importMode(self: *Self, buffer: Handle, consumer: Owner, read_only: bool) Error!Handle {
            if (!consumer.valid()) return error.Invalid;
            const object = try self.findObject(buffer);
            if (object.phase != .live or !object.producer_open) return error.Closed;
            const slot = self.freeReference() orelse return error.Capacity;
            const reference = try self.nextHandle(slot);
            self.references[slot] = .{ .handle = reference, .buffer = buffer, .owner = consumer, .read_only = read_only };
            object.references += 1;
            return reference;
        }

        pub fn describe(self: *Self, reference: Handle, owner: Owner) Error!layout.Descriptor {
            return (try self.referencedObject(reference, owner)).descriptor;
        }

        // Sharing names a live reference, preserving immutable-export mode.
        // A bare diagnostic object ID is not an importable reference.
        pub fn share(self: *Self, handle: Handle, consumer: Owner) Error!Handle {
            if (handle.id == 0 or handle.id > reference_capacity or handle.generation == 0) return error.Stale;
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
            if (object.phase != .live) return error.Closed;
            if (writes(access)) {
                for (&self.references) |item| if (item.handle.id != 0 and item.buffer.eql(object.handle) and item.read_only) {
                    return error.Busy;
                };
            }
            if (!layout.spanFits(object.descriptor.bytes, offset, bytes)) return error.Invalid;
            const backing = object.backing orelse return error.Busy;
            const required: u32 = switch (access) {
                .cpu_read => layout.Usage.cpu_read,
                .cpu_write => layout.Usage.cpu_write,
                .device_read, .queue_read => layout.Usage.transfer_source,
                .device_write, .queue_write => layout.Usage.transfer_target | layout.Usage.render,
                .scanout => layout.Usage.scanout,
                .device_mapping => 0,
            };
            if (required != 0 and (object.descriptor.usage & required) == 0) return error.Unsupported;
            if ((access == .cpu_read or access == .cpu_write) and backing.cpu_address == 0) return error.Unsupported;
            if (access == .cpu_read and backing.cache == .write_combining) return error.Unsupported;
            for (&self.leases) |lease| {
                if (lease.handle.id != 0 and lease.buffer.eql(object.handle) and conflicts(lease.access, access)) return error.Busy;
            }
            const slot = self.freeLease() orelse return error.Capacity;
            const token = try self.nextHandle(slot);
            self.leases[slot] = .{ .handle = token, .buffer = object.handle, .owner = owner, .access = access, .range = .{ .offset = offset, .bytes = bytes } };
            object.leases += 1;
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
            if (object.phase != .live or object.backing == null) return error.Closed;
            const slot = self.freeReference() orelse return error.Capacity;
            const reference = try self.nextHandle(slot);
            self.references[slot] = .{ .handle = reference, .buffer = object.handle, .owner = driver, .mapping_only = true };
            object.references += 1;
            return reference;
        }

        // GPU/DMA/scanout leases need an engine/TLB completion or a proven
        // stop. A timeout, cancel request, or producer death is not that proof.
        pub fn endUse(self: *Self, handle: Handle, owner: Owner, device_quiesced: bool) Error!void {
            const lease = try self.findLease(handle, owner);
            if (isDevice(lease.access) and !device_quiesced) return error.Busy;
            const object = try self.findObject(lease.buffer);
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

        // Called only after the program lifecycle has stopped every CPU task.
        // Imported references of other consumers and all device uses survive.
        pub fn stoppedOwner(self: *Self, owner: Owner) void {
            for (&self.objects) |*object| {
                if (object.phase != .empty and object.producer.eql(owner)) object.producer_open = false;
            }
            for (&self.references) |*reference| {
                if (reference.handle.id == 0 or !reference.owner.eql(owner)) continue;
                const object = self.findObject(reference.buffer) catch unreachable;
                reference.* = .{};
                object.references -= 1;
            }
            for (&self.leases) |*lease| {
                if (lease.handle.id == 0 or !lease.owner.eql(owner) or isDevice(lease.access)) continue;
                const object = self.findObject(lease.buffer) catch unreachable;
                lease.* = .{};
                object.leases -= 1;
            }
            for (&self.objects) |*object| if (object.phase != .empty) {
                self.maybeRelease(object);
            };
        }

        // Returning a ticket does not release its budget or slot. The caller
        // must acknowledge successful VM/TLB or backend destruction outside
        // the metadata lock. Failure remains discoverable and retryable.
        pub fn pendingReleaseForOwner(self: *const Self, producer: Owner) bool {
            for (&self.objects) |object| {
                if ((object.phase == .releasing or object.phase == .destroying) and object.producer.eql(producer)) return true;
            }
            return false;
        }

        pub fn pendingRelease(self: *Self) ?Release {
            for (&self.objects) |*object| if (object.phase == .releasing) {
                const backing = object.backing orelse continue;
                if (object.release_attempt == std.math.maxInt(u64)) continue;
                object.release_attempt += 1;
                object.phase = .destroying;
                return .{ .buffer = object.handle, .backing = backing, .attempt = object.release_attempt };
            };
            return null;
        }

        pub fn finishRelease(self: *Self, ticket: Release, released: bool) Error!void {
            const object = try self.findObject(ticket.buffer);
            if (object.release_attempt != ticket.attempt) return error.Stale;
            if (object.phase != .destroying or object.references != 0 or object.leases != 0) return error.Busy;
            const backing = object.backing orelse return error.Stale;
            if (backing.cookie != ticket.backing.cookie) return error.Stale;
            if (!released) {
                object.phase = .releasing;
                return error.Busy;
            }
            self.committed_bytes -= object.allocation_bytes;
            object.* = .{};
        }

        pub fn retainsDriver(self: *const Self, owner: Owner) bool {
            for (&self.objects) |object| if (object.backing) |backing| if (backing.driver) |driver| {
                if (driver.eql(owner)) return true;
            };
            for (&self.leases) |lease| {
                if (lease.handle.id != 0 and lease.owner.eql(owner) and isDevice(lease.access)) return true;
            }
            return false;
        }

        pub fn stats(self: *const Self) Stats {
            var result = Stats{ .bytes = self.committed_bytes };
            for (&self.objects) |object| {
                if (object.phase == .empty) continue;
                result.objects += 1;
                result.references += object.references;
                result.leases += object.leases;
                if (!object.producer_open or object.phase == .releasing or object.phase == .destroying) result.retained_bytes += object.allocation_bytes;
            }
            return result;
        }

        fn maybeRelease(_: *Self, object: *Object) void {
            if (object.phase == .live and object.references == 0 and object.leases == 0) object.phase = .releasing;
        }
        fn nextHandle(self: *Self, slot: usize) Error!Handle {
            if (self.serial == std.math.maxInt(u64) or slot >= std.math.maxInt(u32)) return error.Exhausted;
            self.serial += 1;
            return .{ .id = @intCast(slot + 1), .generation = self.serial };
        }
        fn findObject(self: *Self, handle: Handle) Error!*Object {
            if (handle.id == 0 or handle.id > object_capacity or handle.generation == 0) return error.Stale;
            const item = &self.objects[handle.id - 1];
            if (item.phase == .empty or !item.handle.eql(handle)) return error.Stale;
            return item;
        }
        fn findReference(self: *Self, handle: Handle, owner: Owner) Error!*Reference {
            if (handle.id == 0 or handle.id > reference_capacity or handle.generation == 0) return error.Stale;
            const item = &self.references[handle.id - 1];
            if (!item.handle.eql(handle)) return error.Stale;
            if (!item.owner.eql(owner)) return error.WrongOwner;
            return item;
        }
        fn referencedObject(self: *Self, handle: Handle, owner: Owner) Error!*Object {
            return self.findObject((try self.findReference(handle, owner)).buffer);
        }
        fn findLease(self: *Self, handle: Handle, owner: Owner) Error!*Lease {
            if (handle.id == 0 or handle.id > lease_capacity or handle.generation == 0) return error.Stale;
            const item = &self.leases[handle.id - 1];
            if (!item.handle.eql(handle)) return error.Stale;
            if (!item.owner.eql(owner)) return error.WrongOwner;
            return item;
        }
        fn freeObject(self: *const Self) ?usize {
            for (&self.objects, 0..) |item, index| if (item.phase == .empty) {
                return index;
            };
            return null;
        }
        fn freeReference(self: *const Self) ?usize {
            for (&self.references, 0..) |item, index| if (item.handle.id == 0) {
                return index;
            };
            return null;
        }
        fn freeLease(self: *const Self) ?usize {
            for (&self.leases, 0..) |item, index| if (item.handle.id == 0) {
                return index;
            };
            return null;
        }
    };
}

fn isDevice(access: Access) bool {
    return access != .cpu_read and access != .cpu_write;
}
fn conflicts(first: Access, second: Access) bool {
    if (first == .device_mapping or second == .device_mapping) return false;
    if (queued(first) and queued(second)) return false;
    return writes(first) or writes(second);
}
fn queued(access: Access) bool {
    return access == .queue_read or access == .queue_write;
}
fn writes(access: Access) bool {
    return access == .cpu_write or access == .device_write or access == .queue_write;
}

test "producer exit preserves imported pixels and outstanding scanout until explicit release" {
    const t = std.testing;
    var store = Table(4, 12, 12){ .budget_bytes = 16384, .producer_budget_bytes = 8192 };
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
    store.stoppedOwner(producer);
    try t.expectError(error.Closed, store.import(created.buffer, producer));
    try t.expectError(error.Stale, store.describe(created.reference, producer));
    try t.expectEqual(@as(u8, 0x5A), @as([*]const u8, @ptrFromInt(first_map.backing.cpu_address))[4095]);
    try store.endUse(first_map.lease, consumer, false);
    try store.endUse(second_map.lease, consumer, false);
    try store.drop(imported, consumer);
    store.stoppedOwner(driver);
    try t.expect(store.retainsDriver(driver));
    try t.expectEqual(@as(?Release, null), store.pendingRelease());
    try t.expectError(error.Busy, store.endUse(scanout.lease, driver, false));
    try store.endUse(scanout.lease, driver, true);
    const release = store.pendingRelease().?;
    try t.expectEqual(@as(?Release, null), store.pendingRelease());
    try t.expectError(error.Busy, store.finishRelease(release, false));
    try t.expectEqual(@as(u64, 4096), store.stats().bytes);
    const retry = store.pendingRelease().?;
    try t.expectError(error.Stale, store.finishRelease(release, true));
    try store.finishRelease(retry, true);
    try t.expectEqual(@as(u64, 0), store.stats().bytes);
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
