// The caller holds the shared BO/queue metadata owner. Admission copies only
// descriptors and reserves existing backing; no pixels or device commands run
// here. In-use backing belongs to the queue, independently of producer death.
const std = @import("std");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
const queue = @import("queue_state.zig");
const abi = @import("r4os_kernel_contract");
pub const owner = lifetime.Owner{ .kind = .kernel, .id = 2, .generation = 1 };
pub const display_owner = lifetime.Owner{ .kind = .kernel, .id = 3, .generation = 1 };
pub const Operation = enum(u32) { copy, barrier, upload, copy_rows, render, present };
pub const Request = struct {
    operation: Operation = .copy,
    source: lifetime.Handle = .{},
    target: lifetime.Handle = .{},
    source_offset: u64 = 0,
    target_offset: u64 = 0,
    bytes: u64 = 0,
    row_count: u32 = 0,
    source_pitch: u64 = 0,
    target_pitch: u64 = 0,
    render: abi.GfxRenderCommand = .{},
    memory_binding: ?lifetime.layout.Binding = null,
};
pub const Entry = struct {
    fence: queue.Fence = .{},
    operation: Operation = .barrier,
    uses: [2]?lifetime.Use = .{ null, null },
    bytes: u64 = 0,
    copied: u64 = 0,
    source_offset: u64 = 0,
    target_offset: u64 = 0,
    row_bytes: u64 = 0,
    row_count: u32 = 0,
    source_pitch: u64 = 0,
    target_pitch: u64 = 0,
    render: abi.GfxRenderCommand = .{},
    deadline_ns: u64 = 0,
};
pub const Error = lifetime.Error || queue.Error;

pub fn rowSpan(bytes: u64, rows: u32, pitch: u64) Error!u64 {
    if (bytes == 0 or rows == 0 or pitch < bytes) return error.Invalid;
    return std.math.add(u64, std.math.mul(u64, rows - 1, pitch) catch return error.Overflow, bytes) catch error.Overflow;
}

// Geometric copies refer to logical plane offsets. An opaque modifier never
// grants the kernel knowledge of a physical tiled address or byte extent.
fn planeRows(desc: lifetime.layout.Descriptor, offset: u64, bytes: u64, rows: u32, pitch: u64) Error!void {
    if (desc.format == .bytes) return;
    const multi = desc.format == .nv12 or desc.format == .p010;
    const sample: u64 = switch (desc.format) { .xrgb8888, .argb8888 => 4, .p010 => 2, else => 1 };
    for (desc.planes[0..desc.plane_count], 0..) |plane, index| {
        if (offset < plane.offset or pitch != plane.pitch) continue;
        const relative = offset - plane.offset;
        const x = relative % pitch;
        const y = relative / pitch;
        const columns: u64 = if (multi and index == 1) ((@as(u64, desc.width) + 1) / 2) * 2 else desc.width;
        const height: u64 = if (multi and index == 1) (@as(u64, desc.height) + 1) / 2 else desc.height;
        if (y < height and rows <= height - y and x < columns * sample and bytes <= columns * sample - x) return;
    }
    return error.Invalid;
}

// Called outside the metadata owner with both queue leases still retained.
// Bound copied bytes and discontiguous spans independently; narrow rows must
// not turn one worker slice into an unbounded loop.
pub fn copyChunk(entry: *const Entry, budget: u64, max_spans: u32) u64 {
    const source = entry.uses[0] orelse return 0;
    const target = entry.uses[1] orelse return 0;
    const remaining = @min(budget, entry.bytes - entry.copied);
    var done: u64 = 0;
    var spans: u32 = 0;
    while (done < remaining and spans < max_spans) : (spans += 1) {
        const cursor = entry.copied + done;
        const rows = entry.operation == .copy_rows;
        const contiguous = !rows or (entry.source_pitch == entry.row_bytes and entry.target_pitch == entry.row_bytes);
        const y = if (rows) cursor / entry.row_bytes else 0;
        const x = if (rows) cursor % entry.row_bytes else cursor;
        const count = if (contiguous) remaining - done else @min(remaining - done, entry.row_bytes - x);
        const src: [*]const u8 = @ptrFromInt(source.backing.cpu_address + entry.source_offset + y * entry.source_pitch + x);
        const dst: [*]u8 = @ptrFromInt(target.backing.cpu_address + entry.target_offset + y * entry.target_pitch + x);
        @memcpy(dst[0..count], src[0..count]);
        done += count;
    }
    return done;
}

pub fn Resources(comptime capacity: usize) type {
    return struct {
        const Self = @This();
        entries: [capacity]Entry = .{Entry{}} ** capacity,

        fn ordered(self: *Self, state: anytype, timeline: u64, dependencies: []const queue.Fence, object: lifetime.Handle, write: bool) Error!void {
            for (&self.entries) |prior| {
                if (prior.fence.slot == 0) continue;
                const status = try state.query(prior.fence);
                if (status.phase == .terminal and !status.device_active) continue;
                for (prior.uses, 0..) |use, j| if (use) |held| {
                    if (!held.buffer.eql(object) or (!write and j == 0)) continue;
                    if (!try state.orders(timeline, dependencies, prior.fence)) return error.Busy;
                };
            }
        }

        fn renderSubmit(self: *Self, state: anytype, buffers: anytype, timeline: u64, producer: lifetime.Owner, submission: queue.Submission, request: Request, now: u64) Error!queue.Fence {
            const config = try state.configuration(timeline, producer);
            if (config.binding.adapter == 0) return error.Unsupported;
            const draw = request.render;
            if (request.bytes != 0 or request.source_offset != 0 or request.target_offset != 0 or draw.reserved0 != 0 or
                draw.kind > abi.gfx_render_kind_sample or draw.filter > abi.gfx_render_filter_bilinear or draw.blend > abi.gfx_render_blend_over or
                draw.transfer > abi.gfx_render_transfer_srgb_encode or draw.opacity > 255) return error.Invalid;
            const sampled = draw.kind == abi.gfx_render_kind_sample;
            if (!sampled and (request.source.id != 0 or request.source.generation != 0 or draw.filter != 0 or draw.transfer != 0 or
                !std.meta.eql(draw.source_rect, abi.GfxRenderRect{}))) return error.Invalid;
            if (sampled and draw.color != 0) return error.Invalid;
            var entry = Entry{ .operation = .render, .render = draw, .deadline_ns = submission.deadline_ns };
            const references = [_]lifetime.Handle{ request.source, request.target };
            var objects: [2]lifetime.Handle = .{ .{}, .{} };
            var extents: [2]u64 = .{ 0, 0 };
            const first: usize = if (sampled) 0 else 1;
            for (first..2) |i| {
                const reference = references[i];
                objects[i] = try buffers.bufferFor(reference, producer);
                const descriptor = try buffers.describe(reference, producer);
                if (!descriptor.binding.portable()) {
                    const binding = request.memory_binding orelse return error.Stale;
                    if (binding.adapter != config.binding.adapter or !std.meta.eql(descriptor.binding, binding)) return error.Stale;
                }
                if (i == 1 and descriptor.usage & lifetime.layout.Usage.render == 0) return error.Unsupported;
                extents[i] = descriptor.bytes;
                try self.ordered(state, timeline, submission.dependencies, objects[i], i == 1);
            }
            if (sampled and objects[0].eql(objects[1])) return error.Unsupported;
            if (sampled) entry.uses[0] = try buffers.reserveQueued(request.source, producer, owner, false, 0, extents[0]);
            errdefer if (entry.uses[0]) |held| buffers.endUse(held.lease, owner, true) catch unreachable;
            entry.uses[1] = try buffers.reserveQueued(request.target, producer, owner, true, 0, extents[1]);
            errdefer buffers.endUse(entry.uses[1].?.lease, owner, true) catch unreachable;
            entry.fence = try state.submit(timeline, producer, submission, now);
            std.debug.assert(self.entries[entry.fence.slot - 1].fence.slot == 0);
            self.entries[entry.fence.slot - 1] = entry;
            return entry.fence;
        }

        pub fn submit(self: *Self, state: anytype, buffers: anytype, timeline: u64, producer: lifetime.Owner, submission: queue.Submission, request: Request, now: u64) Error!queue.Fence {
            const config = try state.configuration(timeline, producer);
            if (request.operation == .present) return self.presentSubmit(state, buffers, timeline, producer, submission, request, now);
            const rows = request.operation == .copy_rows;
            if (!rows and (request.row_count != 0 or request.source_pitch != 0 or request.target_pitch != 0)) return error.Invalid;
            if (request.operation == .render) return self.renderSubmit(state, buffers, timeline, producer, submission, request, now);
            if (!std.meta.eql(request.render, abi.GfxRenderCommand{})) return error.Invalid;
            const source_span = if (rows) try rowSpan(request.bytes, request.row_count, request.source_pitch) else request.bytes;
            const target_span = if (rows) try rowSpan(request.bytes, request.row_count, request.target_pitch) else request.bytes;
            var entry = Entry{ .operation = request.operation, .deadline_ns = submission.deadline_ns,
                .bytes = if (rows) std.math.mul(u64, request.bytes, request.row_count) catch return error.Overflow else request.bytes,
                .source_offset = request.source_offset, .target_offset = request.target_offset,
                .row_bytes = if (rows) request.bytes else 0, .row_count = request.row_count,
                .source_pitch = request.source_pitch, .target_pitch = request.target_pitch };
            if (request.operation == .copy or request.operation == .upload or rows) {
                if (request.bytes == 0) return error.Invalid;
                const source = try buffers.bufferFor(request.source, producer);
                const upload = request.operation == .upload;
                if (upload and (config.binding.adapter == 0 or request.target.id != 0 or request.target.generation != 0 or request.target_offset != 0)) return error.Unsupported;
                const target = if (upload) lifetime.Handle{} else try buffers.bufferFor(request.target, producer);
                // Aliased copies need an explicit memmove/backend contract.
                if (source.eql(target)) return error.Unsupported;
                const references = [_]lifetime.Handle{ request.source, request.target };
                const objects = [_]lifetime.Handle{ source, target };
                var offsets = [_]u64{request.source_offset, request.target_offset};
                var extents = [_]u64{source_span, target_span};
                for (references[0..@as(usize, if (upload) 1 else 2)], 0..) |reference, i| {
                    const descriptor = try buffers.describe(reference, producer);
                    if (!descriptor.binding.portable()) {
                        const binding = request.memory_binding orelse return error.Stale;
                        if (binding.adapter != config.binding.adapter or !std.meta.eql(descriptor.binding, binding)) return error.Stale;
                    }
                    if (!lifetime.layout.spanFits(descriptor.bytes, offsets[i], extents[i])) return error.Invalid;
                    if (rows) try planeRows(descriptor, offsets[i], request.bytes, request.row_count, if (i == 0) request.source_pitch else request.target_pitch);
                    if (descriptor.modifier != 0) {
                        if (!rows or config.binding.adapter == 0) return error.Unsupported;
                        offsets[i] = 0;
                        extents[i] = descriptor.bytes;
                    }
                    try self.ordered(state, timeline, submission.dependencies, objects[i], i == 1);
                }
                entry.uses[0] = try buffers.reserveQueued(request.source, producer, owner, false, offsets[0], extents[0]);
                errdefer buffers.endUse(entry.uses[0].?.lease, owner, true) catch unreachable;
                if (!upload) entry.uses[1] = try buffers.reserveQueued(request.target, producer, owner, true, offsets[1], extents[1]);
                errdefer if (entry.uses[1]) |held| buffers.endUse(held.lease, owner, true) catch unreachable;
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

        // Output admission belongs to DisplayExecution. The queue owns a
        // whole-image read until the driver has copied it into its inactive
        // scanout. The private scanout is never an invented producer handle.
        fn presentSubmit(self: *Self, state: anytype, buffers: anytype, timeline: u64, producer: lifetime.Owner, submission: queue.Submission, request: Request, now: u64) Error!queue.Fence {
            const config = try state.configuration(timeline, producer);
            if (config.binding.adapter == 0 or request.source_offset != 0 or request.target_offset != 0 or
                request.target.id != 0 or request.target.generation != 0 or request.target_pitch != 0 or
                !std.meta.eql(request.render, abi.GfxRenderCommand{})) return error.Invalid;
            const descriptor = try buffers.describe(request.source, producer);
            if (descriptor.format != .xrgb8888 or descriptor.plane_count != 1 or descriptor.planes[0].offset != 0 or
                descriptor.usage & lifetime.layout.Usage.transfer_source == 0 or request.bytes != @as(u64, descriptor.width) * 4 or
                request.row_count != descriptor.height or request.source_pitch != descriptor.planes[0].pitch or
                try rowSpan(request.bytes, request.row_count, request.source_pitch) > descriptor.bytes) return error.Unsupported;
            if (!descriptor.binding.portable()) {
                const binding = request.memory_binding orelse return error.Stale;
                if (binding.adapter != config.binding.adapter or !std.meta.eql(binding, descriptor.binding)) return error.Stale;
            }
            const object = try buffers.bufferFor(request.source, producer);
            try self.ordered(state, timeline, submission.dependencies, object, false);
            const use = try buffers.reserveQueued(request.source, producer, owner, false, 0, descriptor.bytes);
            errdefer buffers.endUse(use.lease, owner, true) catch unreachable;
            const fence = try state.submit(timeline, producer, submission, now);
            std.debug.assert(self.entries[fence.slot - 1].fence.slot == 0);
            self.entries[fence.slot - 1] = .{ .fence = fence, .operation = .present, .uses = .{ use, null },
                .bytes = std.math.mul(u64, request.bytes, request.row_count) catch unreachable,
                .row_bytes = request.bytes, .row_count = request.row_count, .source_pitch = request.source_pitch,
                .deadline_ns = submission.deadline_ns };
            return fence;
        }

        pub fn retain(self: *Self, state: anytype, buffers: anytype, fence: queue.Fence, which: u32, driver: lifetime.Owner) Error!lifetime.Handle {
            if (which > 1) return error.Invalid;
            const status = try state.query(fence);
            if (!status.device_active) return if (status.phase == .queued) error.Busy else error.AlreadyCompleted;
            const entry = &self.entries[fence.slot - 1];
            if (!std.meta.eql(entry.fence, fence)) return error.Stale;
            const use = entry.uses[which] orelse return error.Invalid;
            return buffers.retainQueued(use.lease, owner, driver);
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
        const created = try buffers.begin(producer, .{ .bytes = 4091, .usage = 15 });
        try buffers.publish(created, .{ .cookie = i + 1, .bytes = 4096, .cpu_address = 4096 * (i + 1), .cache = .write_back });
        ref.* = created.reference;
    }
    try t.expectError(error.Invalid, buffers.use(refs[0], producer, .cpu_read, 0, 4096));
    try t.expectError(error.Invalid, buffers.reserveQueued(refs[0], producer, owner, false, 0, 4096));
    const upload = try state.open(producer, .{});
    const render = try state.open(producer, .{});
    const first = try resources.submit(&state, &buffers, upload, producer, .{ .deadline_ns = 100 }, .{ .source = refs[0], .target = refs[1], .bytes = 4091 }, 0);
    const driver = lifetime.Owner{ .kind = .driver, .id = 5, .generation = 17 };
    try t.expectError(error.Busy, resources.retain(&state, &buffers, first, 0, driver));
    try t.expectError(error.Busy, resources.submit(&state, &buffers, render, producer, .{ .deadline_ns = 100 }, .{ .source = refs[1], .target = refs[2], .bytes = 4091 }, 0));
    const second = try resources.submit(&state, &buffers, render, producer, .{ .deadline_ns = 100, .dependencies = &.{first} }, .{ .source = refs[1], .target = refs[2], .bytes = 4091 }, 0);
    try t.expectError(error.Busy, buffers.use(refs[0], producer, .cpu_write, 0, 4091));
    try t.expectError(error.Busy, buffers.use(refs[1], producer, .cpu_read, 0, 4091));
    try t.expectEqualDeep(first, state.takeReady(0).?);
    // Mapping retention must not freeze the source against later ordered
    // writes, and it cannot manufacture an independent execution permission.
    const source_mapping = try resources.retain(&state, &buffers, first, 0, driver);
    const source_dma = try buffers.use(source_mapping, driver, .device_mapping, 0, 4096);
    try t.expectError(error.Invalid, buffers.use(source_mapping, driver, .device_mapping, 0, 4097));
    try t.expectError(error.Unsupported, buffers.share(source_mapping, producer));
    for ([_]lifetime.Access{ .cpu_read, .cpu_write, .device_read, .device_write, .scanout, .queue_read, .queue_write }) |access|
        try t.expectError(error.Unsupported, buffers.use(source_mapping, driver, access, 0, 4096));
    const later_write = try buffers.reserveQueued(refs[0], producer, owner, true, 0, 4091);
    try buffers.endUse(later_write.lease, owner, true);
    buffers.stoppedOwner(producer);
    state.stopped(producer, 1);
    // Public imports remain closed, while the exact active job still owns
    // this target and may hand its mapping to the authenticated driver.
    const target_mapping = try resources.retain(&state, &buffers, first, 1, driver);
    const target_object = try buffers.bufferFor(target_mapping, driver);
    try t.expectError(error.Closed, buffers.import(target_object, driver));
    try t.expect(try buffers.mappingOnly(target_mapping, driver));
    const target_gpu = try buffers.use(target_mapping, driver, .device_mapping, 0, 4096);
    var stale = first;
    stale.binding.reset_generation += 1;
    try t.expectError(error.Stale, resources.retain(&state, &buffers, stale, 0, driver));
    try t.expectError(error.Invalid, resources.retain(&state, &buffers, first, 2, driver));
    try t.expectError(error.Invalid, resources.retain(&state, &buffers, first, 0, producer));
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
    try t.expectError(error.AlreadyCompleted, resources.retain(&state, &buffers, first, 0, driver));
    const active = state.takeRelease().?;
    try resources.release(&buffers, active);
    try state.released(active, true);
    try t.expect(buffers.pendingRelease() == null);
    try buffers.drop(source_mapping, driver);
    try buffers.drop(target_mapping, driver);
    try t.expectError(error.Busy, buffers.endUse(source_dma.lease, driver, false));
    try t.expectError(error.Busy, buffers.endUse(target_gpu.lease, driver, false));
    try t.expect(buffers.pendingRelease() == null);
    try buffers.endUse(source_dma.lease, driver, true);
    try buffers.endUse(target_gpu.lease, driver, true);
    while (buffers.pendingRelease()) |ticket| try buffers.finishRelease(ticket, true);
    try t.expectEqual(@as(u64, 0), buffers.stats().bytes);

    // Actual worker copy primitive: different pitches, partial byte slices,
    // padding sentinels and a dependent readback queued before the upload ends.
    var pixels: [3][4096]u8 align(4096) = .{@as([4096]u8, @splat(0xa5))} ** 3;
    for (0..3) |y| for (0..5) |x| { pixels[0][3 + y * 11 + x] = @intCast(17 + y * 5 + x); };
    for (&refs, 0..) |*ref, i| {
        const created = try buffers.begin(producer, .{ .bytes = 64, .usage = 15 });
        try buffers.publish(created, .{ .cookie = i + 1, .bytes = 4096, .cpu_address = @intFromPtr(&pixels[i]), .cache = .write_back });
        ref.* = created.reference;
    }
    var row_state = queue.Store(2, 8){};
    var row_resources = Resources(8){};
    const forward = try row_state.open(producer, .{});
    const back = try row_state.open(producer, .{});
    var rows: Request = .{ .operation = .copy_rows, .source = refs[0], .target = refs[1],
        .source_offset = 3, .target_offset = 1, .bytes = 5, .row_count = 3, .source_pitch = 11, .target_pitch = 9 };
    const row_fence = try row_resources.submit(&row_state, &buffers, forward, producer, .{ .deadline_ns = 100 }, rows, 0);
    rows.source = refs[1]; rows.target = refs[2]; rows.source_offset = 1; rows.target_offset = 2; rows.source_pitch = 9; rows.target_pitch = 7;
    try t.expectError(error.Busy, row_resources.submit(&row_state, &buffers, back, producer, .{ .deadline_ns = 100 }, rows, 0));
    const back_fence = try row_resources.submit(&row_state, &buffers, back, producer, .{ .deadline_ns = 100, .dependencies = &.{row_fence} }, rows, 0);
    try t.expectError(error.Busy, buffers.use(refs[2], producer, .cpu_read, 0, 64));
    try t.expectEqualDeep(row_fence, row_state.takeReady(1).?);
    try t.expect(row_state.takeReady(1) == null);
    for ([_]queue.Fence{row_fence, back_fence}, 0..) |current, i| {
        if (i != 0) try t.expectEqualDeep(current, row_state.takeReady(2).?);
        const entry = &row_resources.entries[current.slot - 1];
        while (entry.copied < entry.bytes) {
            const count = copyChunk(entry, 7, 1);
            try t.expect(count > 0 and count <= 5);
            entry.copied += count;
        }
        try t.expectEqual(@as(u64, 15), entry.copied);
        try row_state.complete(current, .complete, true, 2);
        const release_ticket = row_state.takeRelease().?;
        try row_resources.release(&buffers, release_ticket);
        try row_state.released(release_ticket, true);
    }
    for (pixels[2], 0..) |pixel, index| {
        const valid = index >= 2 and (index - 2) / 7 < 3 and (index - 2) % 7 < 5;
        const expected: u8 = if (valid) @intCast(17 + ((index - 2) / 7) * 5 + (index - 2) % 7) else 0xa5;
        try t.expectEqual(expected, pixel);
    }
    const read = try buffers.use(refs[2], producer, .cpu_read, 0, 64);
    try buffers.endUse(read.lease, producer, true);
    rows.row_count = 0;
    try t.expectError(error.Invalid, row_resources.submit(&row_state, &buffers, back, producer, .{ .deadline_ns = 100 }, rows, 3));
    rows.row_count = 3; rows.source_pitch = std.math.maxInt(u64);
    try t.expectError(error.Overflow, row_resources.submit(&row_state, &buffers, back, producer, .{ .deadline_ns = 100 }, rows, 3));
    for (refs) |ref| try buffers.drop(ref, producer);
    while (buffers.pendingRelease()) |ticket| try buffers.finishRelease(ticket, true);
    try t.expectEqual(@as(u64, 0), buffers.stats().bytes);
}

test "native upload retains a single source beyond cancellation and producer reference release" {
    try checkImagePresentation();
    const t = std.testing;
    const producer = display_owner;
    var buffers = lifetime.Table(2, 8, 8){ .budget_bytes = 8192, .producer_budget_bytes = 8192 };
    var state = queue.Store(3, 4){};
    var resources = Resources(4){};
    const created = try buffers.begin(producer, .{ .bytes = 4096, .usage = 7 });
    try buffers.publish(created, .{ .cookie = 1, .bytes = 4096, .cpu_address = 4096, .cache = .write_back });
    const software = try state.open(producer, .{});
    const request = Request{ .operation = .upload, .source = created.reference, .bytes = 4096 };
    try t.expectError(error.Unsupported, resources.submit(&state, &buffers, software, producer, .{ .deadline_ns = 100 }, request, 0));
    const device = queue.Binding{ .adapter = 7, .device_generation = 3, .reset_generation = 1 };
    const timeline = try state.open(producer, .{ .binding = device, .milestone = .device_execution });
    const fence = try resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, request, 0);
    try t.expect(resources.entries[fence.slot - 1].uses[1] == null);
    try t.expectError(error.Busy, buffers.use(created.reference, producer, .cpu_write, 0, 4096));
    try t.expectEqualDeep(fence, state.takeReadyFor(device, 1).?);
    try state.cancel(fence, producer, 2);
    try buffers.drop(created.reference, producer);
    try t.expectError(error.Busy, state.complete(fence, .failed, false, 3));
    try t.expect(state.takeRelease() == null and buffers.pendingRelease() == null);
    try state.complete(fence, .failed, true, 4);
    const release = state.takeRelease().?;
    try resources.release(&buffers, release);
    try state.released(release, true);
    try buffers.finishRelease(buffers.pendingRelease().?, true);
    try t.expectEqual(@as(u64, 0), buffers.stats().bytes);
    try t.expectEqual(queue.Result.cancelled, (try state.query(fence)).result);
    // Driver memory epoch 73 is unrelated to queue generation 3. The exact
    // registered owner must match as well; matching an adapter is insufficient.
    const driver = lifetime.Owner{ .kind = .driver, .id = 12, .generation = 4 };
    const memory = lifetime.layout.Binding{ .adapter = 7, .driver_owner = 12, .device_generation = 73 };
    const native = try buffers.beginOwned(driver, .{ .bytes = 4096, .usage = 12, .location = .device_local, .binding = memory }, 79);
    try buffers.commitOwned(native, driver);
    const imported = try buffers.share(native.create.reference, producer);
    const cpu = try buffers.begin(producer, .{ .bytes = 4096, .usage = 15 });
    try buffers.publish(cpu, .{ .cookie = 80, .bytes = 4096, .cpu_address = 4096, .cache = .write_back });
    var copy = Request{ .source = cpu.reference, .target = imported, .bytes = 4096, .memory_binding = memory };
    copy.memory_binding.?.device_generation = device.device_generation;
    try t.expectError(error.Stale, resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, copy, 5));
    copy.memory_binding = memory; copy.memory_binding.?.driver_owner += 1;
    try t.expectError(error.Stale, resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, copy, 5));
    copy.memory_binding = memory;
    const accepted = try resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, copy, 5);
    try t.expectEqualDeep(accepted, state.takeReadyFor(device, 6).?);
    try buffers.drop(imported, producer); try buffers.drop(native.create.reference, driver); try buffers.drop(cpu.reference, producer);
    try t.expect((try buffers.takeOwnedRelease(driver, memory)) == null);
    try state.complete(accepted, .complete, true, 7);
    const done = state.takeRelease().?;
    try resources.release(&buffers, done); try state.released(done, true);
    try buffers.finishOwnedRelease((try buffers.takeOwnedRelease(driver, memory)).?, driver, true);
    try buffers.finishRelease(buffers.pendingSystemRelease().?, true);
    try t.expectEqual(@as(u64, 0), buffers.stats().bytes);

    // Render uses the same queue lifetime without requiring any CPU image
    // mapping. Changed caller state and producer death cannot edit the job.
    var native_refs: [2]lifetime.Handle = undefined;
    var render_refs: [2]lifetime.Handle = undefined;
    for (&native_refs, &render_refs, 0..) |*owned_ref, *ref, i| {
        const image = try buffers.beginOwned(driver, .{ .bytes = 4096, .usage = 28, .location = .device_local, .binding = memory,
            .format = .argb8888, .width = 16, .height = 16, .plane_count = 1, .planes = .{ .{ .pitch = 64 }, .{}, .{}, .{} } }, 100 + i);
        try buffers.commitOwned(image, driver);
        owned_ref.* = image.create.reference;
        ref.* = try buffers.share(owned_ref.*, producer);
    }
    const draw_queue = try state.open(producer, .{ .binding = device, .milestone = .device_execution });
    var draw = Request{ .operation = .render, .target = render_refs[1], .memory_binding = memory,
        .render = .{ .target_rect = .{ .x = -2, .width = 16, .height = 16 }, .scissor = .{ .width = 8, .height = 8 }, .color = 0x80402010, .opacity = 255 } };
    try t.expectError(error.Unsupported, resources.submit(&state, &buffers, software, producer, .{ .deadline_ns = 100 }, draw, 8));
    draw.render.kind = abi.gfx_render_kind_sample;
    draw.render.color = 0; draw.source = render_refs[1];
    try t.expectError(error.Unsupported, resources.submit(&state, &buffers, draw_queue, producer, .{ .deadline_ns = 100 }, draw, 8));
    draw.source = render_refs[0]; draw.render.source_rect = .{ .width = 16, .height = 16 };
    draw.render.reserved0 = 1;
    try t.expectError(error.Invalid, resources.submit(&state, &buffers, draw_queue, producer, .{ .deadline_ns = 100 }, draw, 8));
    draw.render.reserved0 = 0;
    const retained_draw = draw.render;
    const render_fence = try resources.submit(&state, &buffers, draw_queue, producer, .{ .deadline_ns = 100 }, draw, 8);
    draw.render.opacity = 0;
    const held_draw = &resources.entries[render_fence.slot - 1];
    try t.expectEqualDeep(retained_draw, held_draw.render);
    try t.expectEqual(@as(u64, 100), held_draw.deadline_ns);
    try t.expectEqual(@as(u64, 4096), held_draw.uses[1].?.range.bytes);
    // A separately queued reader must explicitly depend on the writer.
    draw.source = render_refs[1]; draw.target = render_refs[0];
    try t.expectError(error.Busy, resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, draw, 8));
    const read_fence = try resources.submit(&state, &buffers, timeline, producer,
        .{ .deadline_ns = 100, .dependencies = &.{render_fence} }, draw, 8);
    try t.expectEqualDeep(render_fence, state.takeReadyFor(device, 9).?);
    for (render_refs) |ref| try buffers.drop(ref, producer);
    for (native_refs) |ref| try buffers.drop(ref, driver);
    const retained_target = try resources.retain(&state, &buffers, render_fence, 1, driver);
    try t.expect(try buffers.mappingOnly(retained_target, driver));
    try t.expect((try buffers.takeOwnedRelease(driver, memory)) == null);
    try state.cancel(render_fence, producer, 10);
    try t.expectError(error.Busy, state.complete(render_fence, .failed, false, 11));
    try t.expect(state.takeRelease() == null);
    try state.complete(render_fence, .failed, true, 12);
    const draw_release = state.takeRelease().?;
    try resources.release(&buffers, draw_release); try state.released(draw_release, true);
    try t.expectEqual(queue.Result.cancelled, (try state.query(render_fence)).result);
    // Failed dependencies terminate the dependent work without dispatch.
    try t.expect(state.takeReadyFor(device, 13) == null);
    const read_release = state.takeRelease().?;
    try t.expectEqualDeep(read_fence, read_release.fence);
    try resources.release(&buffers, read_release); try state.released(read_release, true);
    try buffers.drop(retained_target, driver);
    while (try buffers.takeOwnedRelease(driver, memory)) |ticket| try buffers.finishOwnedRelease(ticket, driver, true);
    try t.expectEqual(@as(u64, 0), buffers.stats().bytes);
}

fn checkImagePresentation() !void {
    const t = std.testing;
    const producer: lifetime.Owner = .{ .kind = .program, .id = 41, .generation = 2 };
    const driver: lifetime.Owner = .{ .kind = .driver, .id = 12, .generation = 4 };
    const memory: lifetime.layout.Binding = .{ .adapter = 7, .driver_owner = 12, .device_generation = 73 };
    const device: queue.Binding = .{ .adapter = 7, .device_generation = 3, .reset_generation = 2 };
    var buffers = lifetime.Table(1, 4, 8){ .budget_bytes = 4096, .producer_budget_bytes = 4096 };
    var state = queue.Store(2, 4){};
    var resources = Resources(4){};
    const native = try buffers.beginOwned(driver, .{ .bytes = 4096, .usage = 28, .location = .device_local, .binding = memory,
        .format = .xrgb8888, .width = 16, .height = 16, .plane_count = 1, .planes = .{ .{ .pitch = 64 }, .{}, .{}, .{} } }, 100);
    try buffers.commitOwned(native, driver);
    const ref = try buffers.share(native.create.reference, producer);
    const rendering = try state.open(producer, .{ .binding = device, .milestone = .device_execution });
    const presenting = try state.open(producer, .{ .binding = device, .milestone = .device_execution });
    const writer = try resources.submit(&state, &buffers, rendering, producer, .{ .deadline_ns = 100 },
        .{ .operation = .render, .target = ref, .memory_binding = memory }, 1);
    var request: Request = .{ .operation = .present, .source = ref, .bytes = 64, .row_count = 16, .source_pitch = 64, .memory_binding = memory };
    request.target_pitch = 64;
    try t.expectError(error.Invalid, resources.submit(&state, &buffers, presenting, producer, .{ .deadline_ns = 100 }, request, 1));
    request.target_pitch = 0;
    try t.expectError(error.Busy, resources.submit(&state, &buffers, presenting, producer, .{ .deadline_ns = 100 }, request, 1));
    const reader = try resources.submit(&state, &buffers, presenting, producer, .{ .deadline_ns = 100, .dependencies = &.{writer} }, request, 1);
    const held = &resources.entries[reader.slot - 1];
    try t.expect(held.uses[1] == null and held.uses[0].?.range.bytes == 4096 and held.bytes == 1024 and held.row_bytes == 64);
    try buffers.drop(ref, producer); try buffers.drop(native.create.reference, driver);
    try t.expectEqualDeep(writer, state.takeReadyFor(device, 2).?);
    try state.complete(writer, .complete, true, 3);
    const written = state.takeRelease().?;
    try resources.release(&buffers, written); try state.released(written, true);
    try t.expectEqualDeep(reader, state.takeReadyFor(device, 4).?);
    const mapped = try resources.retain(&state, &buffers, reader, 0, driver);
    try t.expect(try buffers.mappingOnly(mapped, driver));
    try state.cancel(reader, producer, 5);
    try t.expectError(error.Busy, state.complete(reader, .cancelled, false, 6));
    try t.expect(state.takeRelease() == null and (try buffers.takeOwnedRelease(driver, memory)) == null);
    try state.complete(reader, .complete, true, 7);
    const released = state.takeRelease().?;
    try resources.release(&buffers, released); try state.released(released, true);
    try t.expect((try buffers.takeOwnedRelease(driver, memory)) == null);
    try buffers.drop(mapped, driver);
    try buffers.finishOwnedRelease((try buffers.takeOwnedRelease(driver, memory)).?, driver, true);
    try t.expect(buffers.stats().bytes == 0 and (try state.query(reader)).result == .cancelled);
}
