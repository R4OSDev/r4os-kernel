// Owner-bound MMIO windows. Caller serializes the complete operation using
// its preemptible driver execution guard, never a no-sleep metadata lock.
const std = @import("std");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
pub const Owner = lifetime.Owner;
pub const Handle = lifetime.Handle;
pub const page_size: u64 = 4096;
pub const Policy = enum { other, write_combining, uncached };
pub const Request = struct { resource_base: u64, resource_bytes: u64, offset: u64, bytes: u64, policy: Policy, prefetchable: bool = false };
pub const Extent = struct { physical: u64, bytes: u64 };
pub const Error = error{ Invalid, Overflow, Unsupported, Busy, Capacity, Stale, Exhausted, MapFailed };
pub const Window = struct { handle: Handle, cpu: u64, physical: u64, bytes: u64, policy: Policy, borrowed: bool };

// PAT entries are platform state, not fixed meanings of the PCD/PWT bits.
// UC-minus (7) is deliberately not accepted for register windows requiring UC.
pub fn cacheSelector(pat: u64, memory_type: u8) ?u3 {
    for (0..8) |index| if (@as(u8, @truncate(pat >> @as(u6, @intCast(index * 8)))) == memory_type) return @intCast(index);
    return null;
}

pub fn validate(request: Request) Error!Extent {
    if (request.resource_base == 0 or request.resource_bytes == 0 or request.bytes == 0 or
        (request.resource_base | request.resource_bytes | request.offset | request.bytes) % page_size != 0 or
        !lifetime.layout.spanFits(request.resource_bytes, request.offset, request.bytes)) return error.Invalid;
    if (request.policy == .other or (request.policy == .write_combining and !request.prefetchable)) return error.Unsupported;
    _ = std.math.add(u64, request.resource_base, request.resource_bytes) catch return error.Overflow;
    const start = std.math.add(u64, request.resource_base, request.offset) catch return error.Overflow;
    const end = std.math.add(u64, start, request.bytes) catch return error.Overflow;
    if (end > (@as(u64, 1) << 52)) return error.Unsupported;
    return .{ .physical = start, .bytes = request.bytes };
}

pub fn Manager(comptime Backend: type) type {
    return struct {
        const Self = @This();
        const Record = struct { used: bool = false, owner: Owner = .{ .kind = .driver, .id = 0, .generation = 0 }, window: Window = undefined, mapped: u64 = 0, retiring: bool = false };
        records: [64]Record = .{Record{}} ** 64,
        serial: u64 = 0,
        charged: u64 = 0,
        budget: u64 = 1024 * 1024 * 1024,

        pub fn create(self: *Self, backend: *Backend, owner: Owner, request: Request) Error!Window {
            if (!owner.valid() or owner.kind != .driver) return error.Invalid;
            const extent = try validate(request);
            if (self.charged > self.budget or extent.bytes > self.budget - self.charged) return error.Capacity;
            if (!backend.deviceSpan(extent.physical, extent.bytes)) return error.Unsupported;
            const cpu = backend.cpuAddress(extent.physical, extent.bytes) orelse return error.Overflow;
            var slot: ?usize = null;
            for (&self.records, 0..) |*record, index| {
                if (!record.used) {
                    slot = slot orelse index;
                    continue;
                }
                if (extent.physical < record.window.physical + record.window.bytes and record.window.physical < extent.physical + extent.bytes) return error.Busy;
            }
            const index = slot orelse return error.Capacity;
            if (self.serial == std.math.maxInt(u64)) return error.Exhausted;
            // Never manufacture a second cache alias. Entirely mapped spans
            // must already have the requested attributes; mixed spans fail.
            const existing = backend.policy(cpu);
            if (existing) |policy| if (policy != request.policy) {
                return error.Unsupported;
            };
            var offset: u64 = 0;
            while (offset < extent.bytes) : (offset += page_size) {
                if (backend.policy(cpu + offset) != existing) return error.Unsupported;
            }
            self.serial += 1;
            const window = Window{ .handle = .{ .id = @intCast(index + 1), .generation = self.serial }, .cpu = cpu, .physical = extent.physical, .bytes = extent.bytes, .policy = request.policy, .borrowed = existing != null };
            const record = &self.records[index];
            record.* = .{ .used = true, .owner = owner, .window = window };
            self.charged += extent.bytes;
            if (!window.borrowed) {
                while (record.mapped < extent.bytes) {
                    if (!backend.map(cpu + record.mapped, extent.physical + record.mapped, request.policy)) {
                        record.retiring = true;
                        _ = self.reclaim(backend, record);
                        return error.MapFailed;
                    }
                    record.mapped += page_size;
                }
            }
            return window;
        }

        pub fn release(self: *Self, backend: *Backend, owner: Owner, handle: Handle, quiesced: bool) Error!void {
            if (handle.id == 0 or handle.id > self.records.len) return error.Stale;
            const record = &self.records[handle.id - 1];
            if (!record.used or !record.owner.eql(owner) or !record.window.handle.eql(handle)) return error.Stale;
            if (!quiesced) return error.Busy;
            record.retiring = true;
            if (!self.reclaim(backend, record)) return error.Busy;
        }

        pub fn collect(self: *Self, backend: *Backend, owner: Owner) bool {
            var all_released = true;
            for (&self.records) |*record| {
                if (record.used and record.retiring and record.owner.eql(owner) and !self.reclaim(backend, record)) all_released = false;
            }
            return all_released;
        }
        pub fn retains(self: *const Self, owner: Owner) bool {
            for (&self.records) |*record| if (record.used and record.owner.eql(owner)) return true;
            return false;
        }
        pub fn pending(self: *const Self, owner: Owner) bool {
            for (&self.records) |*record| if (record.used and record.retiring and record.owner.eql(owner)) return true;
            return false;
        }
        fn reclaim(self: *Self, backend: *Backend, record: *Record) bool {
            while (record.mapped != 0) {
                const offset = record.mapped - page_size;
                // A failed shootdown leaves the PTE and record intact. Never
                // subtract charge or discard the only rollback identity.
                if (!backend.unmap(record.window.cpu + offset)) return false;
                record.mapped = offset;
            }
            self.charged -= record.window.bytes;
            record.* = .{};
            return true;
        }
    };
}

test "MMIO windows keep 64-bit offsets, reject cache aliases and retain failed TLB release" {
    try std.testing.expectEqual(@as(?u3, 3), cacheSelector(0x0007_0406_0007_0406, 0));
    try std.testing.expectEqual(@as(?u3, 2), cacheSelector(0x0606_0606_0600_0606, 0));
    try std.testing.expectEqual(@as(?u3, null), cacheSelector(0x0707_0707_0707_0707, 0));
    const Fake = struct {
        entries: [8]?Policy = .{null} ** 8,
        fail_map_after: usize = 8,
        fail_unmap: bool = false,
        pub fn deviceSpan(_: *@This(), _: u64, _: u64) bool {
            return true;
        }
        pub fn cpuAddress(_: *@This(), _: u64, bytes: u64) ?u64 {
            return if (bytes <= 8 * page_size) 0x1000 else null;
        }
        pub fn policy(self: *@This(), address: u64) ?Policy {
            return self.entries[(address - 0x1000) / page_size];
        }
        pub fn map(self: *@This(), address: u64, _: u64, cache: Policy) bool {
            const index = (address - 0x1000) / page_size;
            if (index >= self.fail_map_after) return false;
            self.entries[index] = cache;
            return true;
        }
        pub fn unmap(self: *@This(), address: u64) bool {
            if (self.fail_unmap) return false;
            self.entries[(address - 0x1000) / page_size] = null;
            return true;
        }
    };
    const t = std.testing;
    const owner = Owner{ .kind = .driver, .id = 7, .generation = 11 };
    const request = Request{ .resource_base = 0x1_0000_0000, .resource_bytes = 0x2_0000_0000, .offset = 0x1_0000_1000, .bytes = 4 * page_size, .policy = .uncached };
    var backend = Fake{};
    var manager = Manager(Fake){};
    const window = try manager.create(&backend, owner, request);
    try t.expect(!manager.pending(owner));
    try t.expectEqual(@as(u64, 0x2_0000_1000), window.physical);
    try t.expect(window.cpu != window.physical);
    try t.expectError(error.Busy, manager.release(&backend, owner, window.handle, false));
    backend.fail_unmap = true;
    try t.expectError(error.Busy, manager.release(&backend, owner, window.handle, true));
    try t.expect(manager.pending(owner));
    try t.expect(manager.retains(owner));
    try t.expectEqual(request.bytes, manager.charged);
    backend.fail_unmap = false;
    try t.expect(manager.collect(&backend, owner));
    try t.expect(!manager.pending(owner));
    try t.expectEqual(@as(u64, 0), manager.charged);
    try t.expectError(error.Stale, manager.release(&backend, owner, window.handle, true));
    backend.entries = .{Policy.write_combining} ** 8;
    try t.expectError(error.Unsupported, manager.create(&backend, owner, request));
    backend.entries = .{null} ** 8;
    backend.fail_map_after = 2;
    backend.fail_unmap = true;
    try t.expectError(error.MapFailed, manager.create(&backend, owner, request));
    try t.expect(manager.retains(owner));
    backend.fail_unmap = false;
    try t.expect(manager.collect(&backend, owner));
    try t.expectEqual(@as(u64, 0), manager.charged);
}
