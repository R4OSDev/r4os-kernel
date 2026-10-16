//! Projection of native queued image/resource ownership. The caller retains
//! its original BO owner and disjoint model stores; no duplicate state.
const std = @import("std");
const lifetime = @import("../memory/gfx_buffer_owner.zig");
const queue = @import("queue_state.zig");
const abi = @import("r4os_kernel_contract");
pub const owner = lifetime.Owner{ .kind = .kernel, .id = 2, .generation = 1 };
pub const display_owner = lifetime.Owner{ .kind = .kernel, .id = 3, .generation = 1 };
pub const Operation = enum(u32) { copy, barrier, upload, copy_rows, render, present, render_list, direct_present, render_grid_list, render_color_list, native, render_color_grid_list };
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
    render_list: ?*const abi.GfxRenderList = null,
    grid_list: ?*const abi.GfxRenderGridList = null,
    color_list: ?*const abi.GfxRenderColorList = null,
    color_grid_list: ?*const abi.GfxRenderColorGridList = null,
    memory_binding: ?lifetime.layout.Binding = null,
    display_target: abi.GfxOutputTarget = .{},
};

pub const Error = lifetime.Error || queue.Error;
pub const UseSlot = extern struct {
    has: bool = false, value: lifetime.NativeUse = undefined,
    pub fn get(self: *const UseSlot) ?lifetime.Use { return if (self.has) self.value.value() else null; }
};
pub const Entry = extern struct {
    fence: queue.Fence = .{}, operation: Operation = .barrier, uses: [2]UseSlot = @splat(.{}),
    native_uses: ?[*]UseSlot = null, native_uses_len: usize = 0, bytes: u64 = 0, copied: u64 = 0,
    source_offset: u64 = 0, target_offset: u64 = 0, row_bytes: u64 = 0, row_count: u32 = 0,
    source_pitch: u64 = 0, target_pitch: u64 = 0, render: abi.GfxRenderCommand = .{}, deadline_ns: u64 = 0, display_target: abi.GfxOutputTarget = .{},
    pub fn useAt(self: *const Entry, index: usize) ?lifetime.Use { return self.uses[index].get(); }
    pub fn setNativeUses(self: *Entry, uses: []UseSlot) void { self.native_uses = if (uses.len == 0) null else uses.ptr; self.native_uses_len = uses.len; }
};
pub const NativeRequest = extern struct {
    operation: Operation, source: lifetime.Handle, target: lifetime.Handle, source_offset: u64, target_offset: u64, bytes: u64,
    row_count: u32, source_pitch: u64, target_pitch: u64, render: abi.GfxRenderCommand,
    render_list: ?*const abi.GfxRenderList, grid_list: ?*const abi.GfxRenderGridList, color_list: ?*const abi.GfxRenderColorList,
    color_grid_list: ?*const abi.GfxRenderColorGridList, has_memory: bool, memory: lifetime.layout.Binding, display_target: abi.GfxOutputTarget,
    pub fn from(value: Request) NativeRequest { return .{ .operation = value.operation, .source = value.source, .target = value.target,
        .source_offset = value.source_offset, .target_offset = value.target_offset, .bytes = value.bytes, .row_count = value.row_count,
        .source_pitch = value.source_pitch, .target_pitch = value.target_pitch, .render = value.render, .render_list = value.render_list,
        .grid_list = value.grid_list, .color_list = value.color_list, .color_grid_list = value.color_grid_list,
        .has_memory = value.memory_binding != null, .memory = value.memory_binding orelse .{}, .display_target = value.display_target }; }
};
pub const NativeView = extern struct { entries: [*]Entry, len: usize, lists: [*]abi.GfxRenderList,
    grids: [*][abi.gfx_render_list_capacity]abi.GfxSampleGrid, colors: [*]abi.GfxRenderColorProgram };
const Args = extern struct {
    owner: lifetime.Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, timeline: u64 = 0, now: u64 = 0, deadline: u64 = 0, frame_key: u64 = 0,
    fence: queue.Fence = .{}, release: queue.Release = .{ .fence = .{}, .nonce = 0 }, object: lifetime.Handle = .{}, which: u32 = 0, write: bool = false,
    dependencies: ?[*]const queue.Fence = null, dependencies_len: usize = 0, request: ?*const NativeRequest = null,
    seen: ?[*]bool = null, pending: ?[*]u16 = null,
};
extern fn r4os_rust_display_queue_resources(u32, *const NativeView, ?*const queue.NativeView, ?*const lifetime.NativeView, *const Args, ?*anyopaque) i32;
extern fn r4os_rust_display_row_span(u64, u32, u64, *u64) i32;
extern fn r4os_rust_display_copy_chunk(*const Entry, u64, u32) u64;
fn checked(code: i32) Error!void { return switch (code) {
    0 => {}, 1 => error.Invalid, 2 => error.Overflow, 3 => error.Unsupported, 4 => error.Exhausted, 5 => error.Budget, 6 => error.Capacity,
    7 => error.Stale, 8 => error.Busy, 9 => error.Closed, 10 => error.WrongOwner, 16 => error.AlreadyCompleted, else => unreachable,
}; }
pub fn rowSpan(bytes: u64, rows: u32, pitch: u64) Error!u64 { var out: u64 = undefined; try checked(r4os_rust_display_row_span(bytes, rows, pitch, &out)); return out; }
pub fn copyChunk(entry: *const Entry, budget: u64, max_spans: u32) u64 { return r4os_rust_display_copy_chunk(entry, budget, max_spans); }
extern fn r4os_rust_display_plane_rows(*const lifetime.layout.Descriptor, u64, u64, u32, u64) i32;
fn planeRows(descriptor: lifetime.layout.Descriptor, offset: u64, bytes: u64, rows: u32, pitch: u64) Error!void {
    try checked(r4os_rust_display_plane_rows(&descriptor, offset, bytes, rows, pitch));
}
pub fn Resources(comptime capacity: usize) type {
    return extern struct {
        const Self = @This();
        entries: [capacity]Entry = @splat(.{}), render_lists: [capacity]abi.GfxRenderList = @splat(.{}),
        render_grids: [capacity][abi.gfx_render_list_capacity]abi.GfxSampleGrid = @splat(@splat(.{})),
        render_colors: [capacity]abi.GfxRenderColorProgram = @splat(.{}),
        pub fn nativeView(self: *Self) NativeView { return .{ .entries = &self.entries, .len = capacity, .lists = &self.render_lists, .grids = &self.render_grids, .colors = &self.render_colors }; }
        fn invoke(self: *Self, op: u32, state: anytype, buffers: anytype, input: Args, out: ?*anyopaque) Error!void {
            var args = input; const view = self.nativeView();
            var qview: queue.NativeView = undefined; var qp: ?*const queue.NativeView = null;
            var bview: lifetime.NativeView = undefined; var bp: ?*const lifetime.NativeView = null;
            const count = if (comptime @TypeOf(state) == void) 0 else state.jobs.len;
            var seen: [count]bool = @splat(false); var pending: [count]u16 = undefined;
            args.seen = &seen; args.pending = &pending;
            if (comptime @TypeOf(state) != void) { qview = state.nativeView(); qp = &qview; }
            if (comptime @TypeOf(buffers) != void) {
                if (@hasDecl(@typeInfo(@TypeOf(buffers)).pointer.child, "nativeView")) { bview = buffers.nativeView(); bp = &bview; }
            }
            try checked(r4os_rust_display_queue_resources(op, &view, qp, bp, &args, out));
        }
        pub fn ordered(self: *Self, state: anytype, timeline: u64, dependencies: []const queue.Fence, object: lifetime.Handle, write: bool) Error!void {
            try self.invoke(0, state, {}, .{ .timeline = timeline, .dependencies = dependencies.ptr, .dependencies_len = dependencies.len, .object = object, .write = write }, null);
        }
        pub fn submit(self: *Self, state: anytype, buffers: anytype, timeline: u64, producer: lifetime.Owner, submission: queue.Submission, request: Request, now: u64) Error!queue.Fence {
            const native = NativeRequest.from(request); var out: queue.Fence = undefined;
            try self.invoke(1, state, buffers, .{ .owner = producer, .timeline = timeline, .deadline = submission.deadline_ns, .frame_key = submission.frame_key,
                .dependencies = submission.dependencies.ptr, .dependencies_len = submission.dependencies.len, .request = &native, .now = now }, &out); return out;
        }
        pub fn renderList(self: *Self, state: anytype, fence: queue.Fence) Error!abi.GfxRenderList { var out: abi.GfxRenderList = undefined; try self.invoke(2, state, {}, .{ .fence = fence }, &out); return out; }
        pub fn renderGridList(self: *Self, state: anytype, fence: queue.Fence) Error!abi.GfxRenderGridList { var out: abi.GfxRenderGridList = undefined; try self.invoke(3, state, {}, .{ .fence = fence }, &out); return out; }
        pub fn renderColorList(self: *Self, state: anytype, fence: queue.Fence) Error!abi.GfxRenderColorList { var out: abi.GfxRenderColorList = undefined; try self.invoke(4, state, {}, .{ .fence = fence }, &out); return out; }
        pub fn retain(self: *Self, state: anytype, buffers: anytype, fence: queue.Fence, which: u32, driver: lifetime.Owner) Error!lifetime.Handle { var out: lifetime.Handle = undefined; try self.invoke(5, state, buffers, .{ .fence = fence, .which = which, .owner = driver }, &out); return out; }
        pub fn retainScanout(self: *Self, state: anytype, buffers: anytype, fence: queue.Fence, driver: lifetime.Owner) Error!lifetime.Handle { var out: lifetime.Handle = undefined; try self.invoke(6, state, buffers, .{ .fence = fence, .owner = driver }, &out); return out; }
        pub fn release(self: *Self, buffers: anytype, ticket: queue.Release) Error!void { try self.invoke(7, {}, buffers, .{ .release = ticket }, null); }
    };
}
comptime { if (@sizeOf(UseSlot) != 120) @compileError("Native queued use C layout"); }

test "queued dependencies reserve CPU ownership across producer death and physical completion" {
    const t = std.testing;
    const producer = lifetime.Owner{ .kind = .program, .id = 7, .generation = 9 };
    var buffers = lifetime.Table(3, 8, 16){ .budget_bytes = 12288, .producer_budget_bytes = 12288 };
    const baseline = buffers.stats();
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
    const second = try resources.submit(&state, &buffers, render, producer, .{ .deadline_ns = 200, .dependencies = &.{first} }, .{ .source = refs[1], .target = refs[2], .bytes = 4091 }, 0);
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
    // Missing completion reaches its deadline while a dependent job is
    // queued; device loss and producer death must not substitute a DMA ACK.
    state.expire(100);
    state.deviceLost(first.binding, 101);
    try t.expectEqual(queue.Result.timeout, (try state.query(first)).result);
    try t.expectEqual(queue.Result.device_lost, (try state.query(second)).result);
    try t.expect((try state.query(first)).device_active);
    buffers.stoppedOwner(producer);
    state.stopped(producer, 102);
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
    try state.complete(first, .complete, true, 103);
    try t.expectEqual(queue.Result.timeout, (try state.query(first)).result);
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
    try t.expectEqualDeep(baseline, buffers.stats());
    while (state.takeNotification()) |notification| try state.published(notification);
    var reaped: usize = 0;
    while (state.reapOne() != null) reaped += 1;
    try t.expectEqual(@as(usize, 2), reaped);
    for (&state.queues) |*entry| try t.expectEqual(@as(u64, 0), entry.timeline);
    for (&resources.entries) |*entry| try t.expectEqual(@as(u32, 0), entry.fence.slot);

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
    try t.expectEqualDeep(baseline, buffers.stats());
    row_state.stopped(producer, 4);
    while (row_state.takeNotification()) |notification| try row_state.published(notification);
    reaped = 0;
    while (row_state.reapOne() != null) reaped += 1;
    try t.expectEqual(@as(usize, 2), reaped);
    for (&row_state.queues) |*entry| try t.expectEqual(@as(u64, 0), entry.timeline);
    for (&row_resources.entries) |*entry| try t.expectEqual(@as(u32, 0), entry.fence.slot);
}

test "native upload retains a single source beyond cancellation and producer reference release" {
    const t = std.testing;
    // High-precision image rows keep their real byte width. Accept the
    // final pixel, reject row padding and a rectangle crossing the bottom.
    for ([_]lifetime.layout.Format{ .xrgb2101010, .argb2101010, .abgr16161616f }) |format| {
        const pixel_bytes: u64 = if (format == .abgr16161616f) 8 else 4;
        const descriptor: lifetime.layout.Descriptor = .{ .bytes = 256, .width = 7, .height = 2,
            .format = format, .plane_count = 1, .planes = .{ .{ .offset = 16, .pitch = 64 }, .{}, .{}, .{} } };
        try planeRows(descriptor, 16, 7 * pixel_bytes, 2, 64);
        try planeRows(descriptor, 80 + 6 * pixel_bytes, pixel_bytes, 1, 64);
        try t.expectError(error.Invalid, planeRows(descriptor, 80 + 6 * pixel_bytes, pixel_bytes + 1, 1, 64));
        try t.expectError(error.Invalid, planeRows(descriptor, 80, pixel_bytes, 2, 64));
    }
    for ([_]lifetime.layout.Format{ .xrgb8888, .xrgb2101010, .argb8888, .argb2101010 }) |format| {
        try checkImagePresentation(false, format);
        try checkImagePresentation(true, format);
    }
    try checkGridLifetime();
    try checkColorLifetime();
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
    try t.expect(resources.entries[fence.slot - 1].useAt(1) == null);
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
    var list: abi.GfxRenderList = .{ .count = 2 };
    list.commands[0] = draw.render; list.commands[1] = draw.render; list.commands[1].opacity = 128;
    const expected_list = list;
    draw.operation = .render_list; draw.render_list = &list;
    list.count = 17;
    try t.expectError(error.Invalid, resources.submit(&state, &buffers, draw_queue, producer, .{ .deadline_ns = 100 }, draw, 8));
    list.count = 2; list.commands[2].opacity = 1;
    try t.expectError(error.Invalid, resources.submit(&state, &buffers, draw_queue, producer, .{ .deadline_ns = 100 }, draw, 8));
    list.commands[2] = .{};
    const render_fence = try resources.submit(&state, &buffers, draw_queue, producer, .{ .deadline_ns = 100 }, draw, 8);
    try t.expectError(error.Invalid, resources.renderList(&state, render_fence));
    list.commands[1].opacity = 17;
    draw.render.opacity = 0;
    const held_draw = &resources.entries[render_fence.slot - 1];
    try t.expectEqualDeep(retained_draw, held_draw.render);
    try t.expectEqual(@as(u64, 100), held_draw.deadline_ns);
    try t.expectEqual(@as(u64, 4096), held_draw.useAt(1).?.range.bytes);
    // A separately queued reader must explicitly depend on the writer.
    draw.operation = .render; draw.render_list = null;
    draw.source = render_refs[1]; draw.target = render_refs[0];
    try t.expectError(error.Busy, resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, draw, 8));
    const read_fence = try resources.submit(&state, &buffers, timeline, producer,
        .{ .deadline_ns = 100, .dependencies = &.{render_fence} }, draw, 8);
    try t.expectEqualDeep(render_fence, state.takeReadyFor(device, 9).?);
    try t.expectEqualDeep(expected_list, try resources.renderList(&state, render_fence));
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

fn checkGridLifetime() !void {
    const t = std.testing;
    const producer: lifetime.Owner = .{ .kind = .program, .id = 41, .generation = 2 };
    const device: queue.Binding = .{ .adapter = 7, .device_generation = 3, .reset_generation = 2 };
    var buffers = lifetime.Table(2, 4, 8){ .budget_bytes = 8192, .producer_budget_bytes = 8192 };
    var state = queue.Store(1, 2){}; var resources = Resources(2){};
    var refs: [2]lifetime.Handle = undefined;
    for (&refs, 0..) |*ref, i| {
        const created = try buffers.begin(producer, .{ .bytes = 4096, .usage = 31, .format = .xrgb8888,
            .width = 16, .height = 16, .plane_count = 1, .planes = .{ .{ .pitch = 64 }, .{}, .{}, .{} } });
        try buffers.publish(created, .{ .cookie = i + 1, .bytes = 4096, .cpu_address = (i + 1) * 4096, .cache = .write_back });
        ref.* = created.reference;
    }
    const timeline = try state.open(producer, .{ .binding = device, .milestone = .device_execution });
    var list: abi.GfxRenderGridList = .{ .count = 1 };
    list.commands[0] = .{ .kind = abi.gfx_render_kind_sample, .opacity = 255,
        .source_rect = .{ .width = 16, .height = 16 }, .target_rect = .{ .width = 16, .height = 16 }, .scissor = .{ .width = 16, .height = 16 } };
    list.grids[0] = .{ .enabled = 1, .rotation = 1, .scale = 180, .pixel_width = 16, .pixel_height = 16,
        .viewport_width = 16, .viewport_height = 16, .guest_width = 16, .guest_height = 16 };
    const request: Request = .{ .operation = .render_grid_list, .source = refs[0], .target = refs[1], .render = list.commands[0], .grid_list = &list };
    list.grids[0].scale = 0;
    try t.expectError(error.Invalid, resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, request, 1));
    list.grids[0].scale = 180;
    const expected = list;
    const fence = try resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, request, 1);
    try t.expectError(error.Invalid, resources.renderGridList(&state, fence));
    list.grids[0].scale = 240; list.commands[0].opacity = 0;
    try t.expectEqualDeep(fence, state.takeReadyFor(device, 2).?);
    try t.expectEqualDeep(expected, try resources.renderGridList(&state, fence));
    try t.expectError(error.Invalid, resources.renderList(&state, fence));
    try state.cancel(fence, producer, 3);
    for (refs) |ref| try buffers.drop(ref, producer);
    try t.expectError(error.Busy, state.complete(fence, .failed, false, 4));
    try t.expect(buffers.pendingSystemRelease() == null and state.takeRelease() == null);
    try state.complete(fence, .failed, true, 5);
    const release = state.takeRelease().?;
    try resources.release(&buffers, release); try state.released(release, true);
    while (buffers.pendingSystemRelease()) |ticket| try buffers.finishRelease(ticket, true);
    try t.expectEqual(@as(u64, 0), buffers.stats().bytes);
}

fn checkColorLifetime() !void {
    try checkColorLifetimeKind(false);
    try checkColorLifetimeKind(true);
}
fn checkColorLifetimeKind(combined: bool) !void {
    const t = std.testing;
    const producer: lifetime.Owner = .{ .kind = .program, .id = 41, .generation = 2 };
    const device: queue.Binding = .{ .adapter = 7, .device_generation = 3, .reset_generation = 2 };
    var buffers = lifetime.Table(2, 4, 8){ .budget_bytes = 8192, .producer_budget_bytes = 8192 };
    var state = queue.Store(1, 2){}; var resources = Resources(2){};
    var refs: [2]lifetime.Handle = undefined;
    for (&refs, 0..) |*ref, i| {
        const created = try buffers.begin(producer, .{ .bytes = 4096, .usage = 31, .format = .xrgb8888,
            .width = 16, .height = 16, .plane_count = 1, .planes = .{ .{ .pitch = 64 }, .{}, .{}, .{} } });
        try buffers.publish(created, .{ .cookie = i + 1, .bytes = 4096, .cpu_address = (i + 1) * 4096, .cache = .write_back });
        ref.* = created.reference;
    }
    const timeline = try state.open(producer, .{ .binding = device, .milestone = .device_execution });
    var list: abi.GfxRenderColorList = .{ .count = 1 };
    list.commands[0] = .{ .kind = abi.gfx_render_kind_sample, .transfer = abi.gfx_render_transfer_color, .opacity = 255,
        .source_rect = .{ .width = 16, .height = 16 }, .target_rect = .{ .width = 16, .height = 16 }, .scissor = .{ .width = 16, .height = 16 } };
    for (&list.program.words, 0..) |*word, i| word.* = @intCast(0x10000 + i);
    var request: Request = .{ .operation = .render_color_list, .source = refs[0], .target = refs[1], .render = list.commands[0], .color_list = &list };
    list.program.reserved0 = 1;
    try t.expectError(error.Invalid, resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, request, 1));
    list.program.reserved0 = 0; request.operation = .render;
    try t.expectError(error.Invalid, resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, request, 1));
    request.operation = .render_color_list;
    const expected = list;
    var full: abi.GfxRenderColorGridList = .{ .count = list.count, .commands = list.commands, .program = list.program };
    full.grids[0] = .{ .enabled = 1, .rotation = 3, .scale = 150, .pixel_width = 16, .pixel_height = 16,
        .viewport_width = 13, .viewport_height = 13, .guest_width = 16, .guest_height = 16 };
    const expected_grid = full.grids[0];
    if (combined) {
        request.operation = .render_color_grid_list; request.color_list = null; request.color_grid_list = &full;
        full.grids[0].scale = 0;
        try t.expectError(error.Invalid, resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, request, 1));
        full.grids[0] = expected_grid;
        full.grids[1].enabled = 1;
        try t.expectError(error.Invalid, resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, request, 1));
        full.grids[1] = .{};
    }
    const fence = try resources.submit(&state, &buffers, timeline, producer, .{ .deadline_ns = 100 }, request, 1);
    try t.expectError(error.Invalid, resources.renderColorList(&state, fence));
    list.program.words = @splat(0); list.commands[0].opacity = 0;
    full = .{};
    try t.expectEqualDeep(fence, state.takeReadyFor(device, 2).?);
    try t.expectEqualDeep(expected, try resources.renderColorList(&state, fence));
    if (combined) try t.expectEqualDeep(expected_grid, (try resources.renderGridList(&state, fence)).grids[0]);
    try t.expectError(error.Invalid, resources.renderList(&state, fence));
    try state.cancel(fence, producer, 3);
    for (refs) |ref| try buffers.drop(ref, producer);
    try t.expectError(error.Busy, state.complete(fence, .failed, false, 4));
    try t.expect(buffers.pendingSystemRelease() == null and state.takeRelease() == null);
    try state.complete(fence, .failed, true, 5);
    const release = state.takeRelease().?;
    try resources.release(&buffers, release); try state.released(release, true);
    try t.expectError(error.Invalid, resources.renderColorList(&state, fence));
    while (buffers.pendingSystemRelease()) |ticket| try buffers.finishRelease(ticket, true);
    try t.expectEqual(@as(u64, 0), buffers.stats().bytes);
}

fn checkImagePresentation(direct: bool, format: lifetime.layout.Format) !void {
    const t = std.testing;
    const producer: lifetime.Owner = .{ .kind = .program, .id = 41, .generation = 2 };
    const driver: lifetime.Owner = .{ .kind = .driver, .id = 12, .generation = 4 };
    const memory: lifetime.layout.Binding = .{ .adapter = 7, .driver_owner = 12, .device_generation = 73 };
    const device: queue.Binding = .{ .adapter = 7, .device_generation = 3, .reset_generation = 2 };
    var buffers = lifetime.Table(1, 4, 8){ .budget_bytes = 4096, .producer_budget_bytes = 4096 };
    var state = queue.Store(2, 4){};
    var resources = Resources(4){};
    const native = try buffers.beginOwned(driver, .{ .bytes = 4096, .usage = if (direct) 60 else 28, .location = .device_local, .binding = memory,
        .format = format, .width = 16, .height = 16, .plane_count = 1, .planes = .{ .{ .pitch = 64 }, .{}, .{}, .{} } }, 100);
    try buffers.commitOwned(native, driver);
    const ref = try buffers.share(native.create.reference, producer);
    const rendering = try state.open(producer, .{ .binding = device, .milestone = .device_execution });
    const presenting = try state.open(producer, .{ .binding = device, .milestone = .device_execution });
    const writer = try resources.submit(&state, &buffers, rendering, producer, .{ .deadline_ns = 100 },
        .{ .operation = .render, .target = ref, .memory_binding = memory }, 1);
    const display_target: abi.GfxOutputTarget = .{ .adapter_id = device.adapter, .device_generation = device.device_generation,
        .connector_id = 2, .connection_generation = 4, .display_generation = 9, .head_id = 1 };
    var request: Request = .{ .operation = if (direct) .direct_present else .present, .source = ref, .bytes = 64, .row_count = 16,
        .source_pitch = 64, .memory_binding = memory, .display_target = display_target };
    request.target_pitch = 64;
    try t.expectError(error.Invalid, resources.submit(&state, &buffers, presenting, producer, .{ .deadline_ns = 100 }, request, 1));
    request.target_pitch = 0;
    if (format == .argb8888 or format == .argb2101010) {
        const before = buffers.stats();
        try t.expectError(error.Unsupported, resources.submit(&state, &buffers, presenting, producer,
            .{ .deadline_ns = 100, .dependencies = &.{writer} }, request, 1));
        try t.expectEqualDeep(before, buffers.stats());
        try buffers.drop(ref, producer); try buffers.drop(native.create.reference, driver);
        try t.expectEqualDeep(writer, state.takeReadyFor(device, 2).?);
        try state.complete(writer, .complete, true, 3);
        const written = state.takeRelease().?;
        try resources.release(&buffers, written); try state.released(written, true);
        try buffers.finishOwnedRelease((try buffers.takeOwnedRelease(driver, memory)).?, driver, true);
        try t.expect(buffers.stats().bytes == 0);
        return;
    }
    request.bytes = 63;
    try t.expectError(error.Unsupported, resources.submit(&state, &buffers, presenting, producer,
        .{ .deadline_ns = 100, .dependencies = &.{writer} }, request, 1));
    request.bytes = 64;
    try t.expectError(error.Busy, resources.submit(&state, &buffers, presenting, producer, .{ .deadline_ns = 100 }, request, 1));
    const reader = try resources.submit(&state, &buffers, presenting, producer, .{ .deadline_ns = 100, .dependencies = &.{writer} }, request, 1);
    request.display_target.head_id = 3;
    try t.expectEqualDeep(display_target, resources.entries[reader.slot - 1].display_target);
    const held = &resources.entries[reader.slot - 1];
    try t.expect(held.useAt(1) == null and held.useAt(0).?.range.bytes == 4096 and held.bytes == 1024 and held.row_bytes == 64);
    try buffers.drop(ref, producer); try buffers.drop(native.create.reference, driver);
    try t.expectEqualDeep(writer, state.takeReadyFor(device, 2).?);
    try state.complete(writer, .complete, true, 3);
    const written = state.takeRelease().?;
    try resources.release(&buffers, written); try state.released(written, true);
    try t.expectEqualDeep(reader, state.takeReadyFor(device, 4).?);
    const mapped = if (direct) try resources.retainScanout(&state, &buffers, reader, driver)
        else try resources.retain(&state, &buffers, reader, 0, driver);
    try t.expect((try buffers.mappingOnly(mapped, driver)) != direct);
    var display_read: ?lifetime.Use = null;
    if (direct) {
        try t.expect(try buffers.readOnly(mapped, driver));
        try t.expectError(error.Unsupported, buffers.use(mapped, driver, .device_write, 0, 4096));
        display_read = try buffers.use(mapped, driver, .device_read, 0, 4096);
        try state.beginScanout(reader, 4);
    }
    try state.cancel(reader, producer, 5);
    try t.expectError(error.Busy, state.complete(reader, .cancelled, false, 6));
    try t.expect(state.takeRelease() == null and (try buffers.takeOwnedRelease(driver, memory)) == null);
    if (display_read) |use| try buffers.endUse(use.lease, driver, true);
    try state.complete(reader, .complete, true, 7);
    const released = state.takeRelease().?;
    try resources.release(&buffers, released); try state.released(released, true);
    try t.expect((try buffers.takeOwnedRelease(driver, memory)) == null);
    try buffers.drop(mapped, driver);
    try buffers.finishOwnedRelease((try buffers.takeOwnedRelease(driver, memory)).?, driver, true);
    try t.expect(buffers.stats().bytes == 0 and (try state.query(reader)).result == (if (direct) queue.Result.complete else .cancelled));
}
