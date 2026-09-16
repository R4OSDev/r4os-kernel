//! Resident opaque command metadata and canonical VA/BO execution ownership.
//! GPU methods and placement remain with the registered userland driver.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const lifetime = buffers.lifetime;
const virtual = @import("../kernel/gfx_virtual.zig");
const heap = @import("../memory/heap.zig");
const queue_resources = @import("queue_resources.zig");
pub const Error = queue_resources.Error || buffers.Error || virtual.Error;
const Resource = struct {
    input: abi.GfxNativeResource,
    held: ?virtual.Execution = null,
    object: lifetime.Handle = .{},
    offset: u64 = 0,
    bytes: u64 = 0,
    write: bool = false,
    leader: usize = 0,
};
pub const Job = struct {
    allocation: []u8,
    info: abi.GfxNativeJobInfo,
    resources: []Resource,
    uses: []?lifetime.Use,
    order: []usize,
    commands: []u8,

    pub fn create(input: abi.GfxNativeSubmission) buffers.Error!*Job {
        if (input.version != 1 or input.size != @sizeOf(abi.GfxNativeSubmission) or input.reserved0 != 0 or
            input.revision == 0 or (input.interface_id_lo == 0 and input.interface_id_hi == 0) or
            (input.commands == 0) != (input.command_bytes == 0) or (input.resources == 0) != (input.resource_count == 0) or
            input.resources % @alignOf(abi.GfxNativeResource) != 0 or
            input.commands > std.math.maxInt(u64) - @as(u64, input.command_bytes) or
            input.resources > std.math.maxInt(u64) - @as(u64, input.resource_count) * @sizeOf(abi.GfxNativeResource)) return error.Invalid;
        const resource_offset = std.mem.alignForward(usize, @sizeOf(Job), @alignOf(Resource));
        const use_offset = std.mem.alignForward(usize, resource_offset + @as(usize, input.resource_count) * @sizeOf(Resource), @alignOf(?lifetime.Use));
        const order_offset = std.mem.alignForward(usize, use_offset + @as(usize, input.resource_count) * @sizeOf(?lifetime.Use), @alignOf(usize));
        const command_offset = order_offset + @as(usize, input.resource_count) * @sizeOf(usize);
        const allocation = heap.alloc(command_offset + input.command_bytes, @max(@alignOf(Job), @alignOf(Resource))) orelse return error.OutOfMemory;
        const job: *Job = @ptrCast(@alignCast(allocation.ptr));
        const items: [*]Resource = @ptrCast(@alignCast(allocation.ptr + resource_offset));
        const uses: [*]?lifetime.Use = @ptrCast(@alignCast(allocation.ptr + use_offset));
        const order: [*]usize = @ptrCast(@alignCast(allocation.ptr + order_offset));
        job.* = .{ .allocation = allocation, .info = .{ .interface_id_lo = input.interface_id_lo,
            .interface_id_hi = input.interface_id_hi, .revision = input.revision,
            .command_bytes = input.command_bytes, .resource_count = input.resource_count },
            .resources = items[0..input.resource_count], .uses = uses[0..input.resource_count],
            .order = order[0..input.resource_count], .commands = allocation[command_offset..] };
        @memset(job.uses, null);
        if (input.resource_count != 0) {
            const from: [*]const abi.GfxNativeResource = @ptrFromInt(input.resources);
            for (job.resources, 0..) |*item, i| item.* = .{ .input = from[i] };
        }
        if (input.command_bytes != 0) {
            const from: [*]const u8 = @ptrFromInt(input.commands);
            @memcpy(job.commands, from[0..input.command_bytes]);
        }
        return job;
    }
    /// Caller holds the shared owner and prepared enough lease slots. No
    /// callbacks, heap work, user pointers or hardware operations in this scope.
    pub fn acquireLocked(self: *Job, producer: lifetime.Owner, binding: lifetime.layout.Binding,
        state: anytype, resources: anytype, timeline: u64, dependencies: anytype) Error!void
    {
        for (self.resources, 0..) |*item, index| {
            const held = try virtual.retainExecutionLocked(producer, item.input, binding);
            item.held = held;
            item.object = try buffers.store.bufferFor(held.reference, held.driver);
            const desc = try buffers.store.describe(held.reference, held.driver);
            if (!desc.binding.portable() and !std.meta.eql(desc.binding, binding)) return error.Stale;
            item.offset = held.entry.request.byte_offset;
            item.bytes = held.entry.request.byte_length;
            if (!lifetime.layout.spanFits(desc.bytes, item.offset, item.bytes)) return error.Invalid;
            item.write = item.input.access == 1;
            item.leader = index;
            self.order[index] = index;
        }
        // Sort a resident index, preserving the driver's original binding
        // order. Large lists must not deduplicate through quadratic scans.
        std.sort.heap(usize, self.order, self.resources, struct {
            fn less(items: []Resource, left: usize, right: usize) bool {
                const lhs = items[left].object; const rhs = items[right].object;
                return if (lhs.id != rhs.id) lhs.id < rhs.id else lhs.generation < rhs.generation;
            }
        }.less);
        for (self.order, 0..) |index, position| {
            if (position == 0) continue;
            const item = &self.resources[index];
            const previous = &self.resources[self.order[position - 1]];
            if (previous.object.eql(item.object)) {
                item.leader = previous.leader;
                const leader = &self.resources[item.leader];
                const end = @max(leader.offset + leader.bytes, item.offset + item.bytes);
                leader.offset = @min(leader.offset, item.offset);
                leader.bytes = end - leader.offset;
                leader.write = leader.write or item.write;
            }
        }
        for (self.resources, 0..) |*item, index| {
            if (item.leader != index) continue;
            try resources.ordered(state, timeline, dependencies, item.object, item.write);
            const held = item.held.?;
            self.uses[index] = try buffers.store.reserveQueued(held.reference, held.driver, queue_resources.owner,
                item.write, item.offset, item.bytes);
        }
    }
    pub fn rollbackLocked(self: *Job) void {
        for (self.uses) |*slot| if (slot.*) |use| {
            buffers.store.endUse(use.lease, queue_resources.owner, true) catch unreachable;
            slot.* = null;
        };
        self.releaseBindingsLocked();
    }
    pub fn releaseBindingsLocked(self: *Job) void {
        for (self.uses) |use| std.debug.assert(use == null);
        for (self.resources) |*item| if (item.held) |held| {
            virtual.releaseExecutionLocked(held);
            item.held = null;
        };
    }
    pub fn destroy(self: *Job) void {
        for (self.uses) |use| std.debug.assert(use == null);
        for (self.resources) |item| std.debug.assert(item.held == null);
        const allocation = self.allocation;
        std.debug.assert(heap.free(allocation) == .ok);
    }
};
