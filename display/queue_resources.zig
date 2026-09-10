// The caller holds the shared BO/queue metadata owner. Admission copies only
// descriptors and reserves existing backing; no pixels or device commands run
// here. In-use backing belongs to the queue, independently of producer death.
const std = @import("std");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
const queue = @import("queue_state.zig");
pub const owner = lifetime.Owner{ .kind = .kernel, .id = 2, .generation = 1 };
pub const Operation = enum(u32) { copy, barrier };
pub const Request = struct {
    operation: Operation = .copy,
    source: lifetime.Handle = .{},
    target: lifetime.Handle = .{},
    source_offset: u64 = 0,
    target_offset: u64 = 0,
    bytes: u64 = 0,
};
pub const Entry = struct {
    fence: queue.Fence = .{},
    operation: Operation = .barrier,
    uses: [2]?lifetime.Use = .{ null, null },
    bytes: u64 = 0,
    copied: u64 = 0,
};
pub const Error = lifetime.Error || queue.Error;

pub fn Resources(comptime capacity: usize) type {
    return struct {
        const Self = @This();
        entries: [capacity]Entry = .{Entry{}} ** capacity,

        pub fn submit(self: *Self, state: anytype, buffers: anytype, timeline: u64, producer: lifetime.Owner, submission: queue.Submission, request: Request, now: u64) Error!queue.Fence {
            const config = try state.configuration(timeline, producer);
            var entry = Entry{ .operation = request.operation, .bytes = request.bytes };
            if (request.operation == .copy) {
                if (request.bytes == 0) return error.Invalid;
                const source = try buffers.bufferFor(request.source, producer);
                const target = try buffers.bufferFor(request.target, producer);
                // Aliased copies need an explicit memmove/backend contract.
                if (source.eql(target)) return error.Unsupported;
                const references = [_]lifetime.Handle{ request.source, request.target };
                const objects = [_]lifetime.Handle{ source, target };
                for (references, 0..) |reference, i| {
                    const descriptor = try buffers.describe(reference, producer);
                    if (!descriptor.binding.portable() and (descriptor.binding.adapter != config.binding.adapter or
                        descriptor.binding.device_generation != config.binding.device_generation)) return error.Stale;
                    for (&self.entries) |prior| {
                        if (prior.fence.slot == 0) continue;
                        const status = try state.query(prior.fence);
                        if (status.phase == .terminal and !status.device_active) continue;
                        for (prior.uses, 0..) |use, j| if (use) |held| {
                            if (!held.buffer.eql(objects[i]) or (i == 0 and j == 0)) continue;
                            if (!try state.orders(timeline, submission.dependencies, prior.fence)) return error.Busy;
                        };
                    }
                }
                entry.uses[0] = try buffers.reserveQueued(request.source, producer, owner, false, request.source_offset, request.bytes);
                errdefer buffers.endUse(entry.uses[0].?.lease, owner, true) catch unreachable;
                entry.uses[1] = try buffers.reserveQueued(request.target, producer, owner, true, request.target_offset, request.bytes);
                errdefer buffers.endUse(entry.uses[1].?.lease, owner, true) catch unreachable;
                // The software adapter only accepts CPU-visible coherent RAM.
                // Native engines may accept other backing via their own bind.
                if (config.binding.adapter == 0) for (entry.uses) |use| {
                    if (use.?.backing.cpu_address == 0 or use.?.backing.cache != .write_back) return error.Unsupported;
                };
                const fence = try state.submit(timeline, producer, submission, now);
                entry.fence = fence;
                std.debug.assert(self.entries[fence.slot - 1].fence.slot == 0);
                self.entries[fence.slot - 1] = entry;
                return fence;
            }
            if (request.source.id != 0 or request.source.generation != 0 or request.target.id != 0 or request.target.generation != 0 or
                request.bytes != 0 or request.source_offset != 0 or request.target_offset != 0) return error.Invalid;
            const fence = try state.submit(timeline, producer, submission, now);
            entry.fence = fence;
            std.debug.assert(self.entries[fence.slot - 1].fence.slot == 0);
            self.entries[fence.slot - 1] = entry;
            return fence;
        }

        pub fn release(self: *Self, buffers: anytype, ticket: queue.Release) Error!void {
            if (ticket.fence.slot == 0 or ticket.fence.slot > capacity) return error.Invalid;
            const entry = &self.entries[ticket.fence.slot - 1];
            if (!std.meta.eql(entry.fence, ticket.fence)) return error.Stale;
            for (&entry.uses) |*use| if (use.*) |held| {
                try buffers.endUse(held.lease, owner, true);
                use.* = null;
            };
            entry.* = .{};
        }
    };
}

test "queued dependencies reserve CPU ownership across producer death and physical completion" {
    const t = std.testing;
    const producer = lifetime.Owner{ .kind = .program, .id = 7, .generation = 9 };
    var buffers = lifetime.Table(3, 8, 16){ .budget_bytes = 12288, .producer_budget_bytes = 12288 };
    var state = queue.Store(2, 8){};
    var resources = Resources(8){};
    var refs: [3]lifetime.Handle = undefined;
    for (&refs, 0..) |*ref, i| {
        const created = try buffers.begin(producer, .{ .bytes = 4096, .usage = 15 });
        try buffers.publish(created, .{ .cookie = i + 1, .bytes = 4096, .cpu_address = 4096 * (i + 1), .cache = .write_back });
        ref.* = created.reference;
    }
    const upload = try state.open(producer, .{});
    const render = try state.open(producer, .{});
    const first = try resources.submit(&state, &buffers, upload, producer, .{ .deadline_ns = 100 }, .{ .source = refs[0], .target = refs[1], .bytes = 4096 }, 0);
    try t.expectError(error.Busy, resources.submit(&state, &buffers, render, producer, .{ .deadline_ns = 100 }, .{ .source = refs[1], .target = refs[2], .bytes = 4096 }, 0));
    const second = try resources.submit(&state, &buffers, render, producer, .{ .deadline_ns = 100, .dependencies = &.{first} }, .{ .source = refs[1], .target = refs[2], .bytes = 4096 }, 0);
    try t.expectError(error.Busy, buffers.use(refs[0], producer, .cpu_write, 0, 4096));
    try t.expectError(error.Busy, buffers.use(refs[1], producer, .cpu_read, 0, 4096));
    try t.expectEqualDeep(first, state.takeReady(0).?);
    buffers.stoppedOwner(producer);
    state.stopped(producer, 1);
    try t.expect(buffers.pendingRelease() == null);
    const queued = state.takeRelease().?;
    try t.expectEqualDeep(second, queued.fence);
    try resources.release(&buffers, queued);
    try state.released(queued, true);
    const target = buffers.pendingRelease().?;
    try buffers.finishRelease(target, true);
    try t.expect(buffers.pendingRelease() == null);
    try t.expectEqual(@as(u64, 8192), buffers.stats().bytes);
    try state.complete(first, .complete, true, 2);
    const active = state.takeRelease().?;
    try resources.release(&buffers, active);
    try state.released(active, true);
    while (buffers.pendingRelease()) |ticket| try buffers.finishRelease(ticket, true);
    try t.expectEqual(@as(u64, 0), buffers.stats().bytes);
}
