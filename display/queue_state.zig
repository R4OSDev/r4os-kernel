comptime { if (@import("builtin").is_test) _ = @import("rust_owner_test"); }
// Graphics submission lifetime only. Command execution, device registers,
// rendering policy and waiter publication live outside this metadata owner.
const std = @import("std");
pub const Owner = @import("../memory/gfx_buffer_owner.zig").Owner;
pub const max_dependencies: usize = 8;
pub const Policy = enum(u32) { fifo, latest_frame };
pub const Milestone = enum(u32) { cpu_stores, device_execution, scanout };
pub const Result = enum(u32) { pending, complete, cancelled, dropped, device_lost, timeout, failed, dependency_failed };
pub const Phase = enum(u32) { queued, running, terminal };
pub const Error = error{ Invalid, Stale, WrongOwner, Busy, Closed, Capacity, Exhausted, AlreadyCompleted };
pub const Binding = extern struct {
    adapter: u32 = 0,
    device_generation: u64 = 1,
    reset_generation: u64 = 1,
};
pub const Fence = extern struct {
    slot: u32 = 0,
    timeline: u64 = 0,
    point: u64 = 0,
    binding: Binding = .{},
};
pub const Config = extern struct {
    binding: Binding = .{},
    policy: Policy = .fifo,
    capacity: u32 = 2,
    milestone: Milestone = .cpu_stores,
};
pub const Submission = struct {
    deadline_ns: u64,
    frame_key: u64 = 0,
    dependencies: []const Fence = &.{},
};
pub const Status = extern struct {
    fence: Fence,
    phase: Phase,
    result: Result,
    milestone: Milestone,
    deadline_ns: u64,
    completed_ns: u64,
    device_active: bool,
    resources_held: bool,
};
pub const Release = extern struct { fence: Fence, nonce: u64 };

const empty_owner = Owner{ .kind = .kernel, .id = 0, .generation = 0 };
pub const Queue = extern struct {
    timeline: u64 = 0, owner: Owner = empty_owner, config: Config = .{}, next_point: u64 = 1,
    closing: bool = false, inflight: u32 = 0, jobs: u32 = 0, has_last_owner: bool = false,
    last_owner_data: Owner = empty_owner, next_owner_queue: usize = 0,
};
pub const Job = extern struct {
    fence: Fence = .{}, queue: usize = 0, phase: Phase = .queued, result: Result = .pending,
    deadline_ns: u64 = 0, completed_ns: u64 = 0, frame_key: u64 = 0, client_reference: bool = true,
    active: bool = false, scanout: bool = false, retire_requested: bool = false, resources_held: bool = true,
    release_nonce: u64 = 0, notification_pending: bool = false, notification_publishing: bool = false,
    waiters: u32 = 0, dependents: u32 = 0, dependencies: [max_dependencies]u16 = @splat(0), dependency_count: usize = 0,
};
const Scalars = extern struct {
    timeline_serial: u64, release_serial: u64, next_queue: usize, has_last_owner: bool,
    last_owner_data: Owner, terminal_publications: u64,
};
pub const NativeView = extern struct { queues: [*]Queue, queues_len: usize, jobs: [*]Job, jobs_len: usize, scalars: *Scalars };
pub const NativeArgs = extern struct {
    owner: Owner = empty_owner, config: Config = .{}, fence: Fence = .{}, binding: Binding = .{},
    timeline: u64 = 0, now: u64 = 0, frame_key: u64 = 0, deadline: u64 = 0, nonce: u64 = 0,
    result: u32 = 0, flag: bool = false, has_binding: bool = false,
    dependencies: ?[*]const Fence = null, dependencies_len: usize = 0, seen: ?[*]bool = null, pending: ?[*]u16 = null,
};
const OptionalFence = extern struct { has: bool, value: Fence };
const OptionalRelease = extern struct { has: bool, value: Release };
extern fn r4os_rust_display_queue_state(*const NativeView, u32, *const NativeArgs, ?*anyopaque) i32;
fn status(code: i32) Error!void { return switch (code) {
    0 => {}, 1 => error.Invalid, 4 => error.Exhausted, 6 => error.Capacity, 7 => error.Stale,
    8 => error.Busy, 9 => error.Closed, 10 => error.WrongOwner, 16 => error.AlreadyCompleted, else => unreachable,
}; }
pub fn Store(comptime queue_capacity: usize, comptime fence_capacity: usize) type {
    if (queue_capacity == 0 or fence_capacity == 0 or fence_capacity > 65535) @compileError("invalid graphics queue capacity");
    return extern struct {
        const Self = @This();
        queues: [queue_capacity]Queue = @splat(.{}), jobs: [fence_capacity]Job = @splat(.{}),
        timeline_serial: u64 = 0, release_serial: u64 = 0, next_queue: usize = 0,
        has_last_owner: bool = false, last_owner_data: Owner = empty_owner, terminal_publications: u64 = 0,
        pub fn nativeView(self: *Self) NativeView {
            return .{ .queues = &self.queues, .queues_len = queue_capacity, .jobs = &self.jobs, .jobs_len = fence_capacity, .scalars = @ptrCast(&self.timeline_serial) };
        }
        fn call(self: *Self, op: u32, args: NativeArgs, out: ?*anyopaque) Error!void {
            const view = self.nativeView(); try status(r4os_rust_display_queue_state(&view, op, &args, out));
        }
        fn result(self: *Self, comptime T: type, op: u32, args: NativeArgs) Error!T {
            var out: T = undefined; try self.call(op, args, &out); return out;
        }
        fn optional(self: *Self, op: u32, args: NativeArgs) ?Fence {
            const out = self.result(OptionalFence, op, args) catch unreachable; return if (out.has) out.value else null;
        }
        pub fn open(self: *Self, owner: Owner, config: Config) Error!u64 { return self.result(u64, 0, .{ .owner = owner, .config = config }); }
        pub fn submit(self: *Self, timeline: u64, owner: Owner, request: Submission, now: u64) Error!Fence {
            return self.result(Fence, 1, .{ .timeline = timeline, .owner = owner, .deadline = request.deadline_ns, .frame_key = request.frame_key, .dependencies = request.dependencies.ptr, .dependencies_len = request.dependencies.len, .now = now });
        }
        pub fn query(self: *Self, fence: Fence) Error!Status { return self.result(Status, 2, .{ .fence = fence }); }
        pub fn configuration(self: *Self, timeline: u64, owner: Owner) Error!Config { return self.result(Config, 3, .{ .timeline = timeline, .owner = owner }); }
        pub fn orders(self: *Self, timeline: u64, dependencies: []const Fence, before: Fence) Error!bool {
            var seen: [fence_capacity]bool = @splat(false); var pending: [fence_capacity]u16 = undefined;
            return self.result(bool, 4, .{ .timeline = timeline, .dependencies = dependencies.ptr, .dependencies_len = dependencies.len, .fence = before, .seen = &seen, .pending = &pending });
        }
        pub fn takeReady(self: *Self, now: u64) ?Fence { return self.takeReadyFor(null, now); }
        pub fn producer(self: *Self, fence: Fence) Error!Owner { return self.result(Owner, 6, .{ .fence = fence }); }
        pub fn takeReadyFor(self: *Self, binding: ?Binding, now: u64) ?Fence { return self.optional(5, .{ .has_binding = binding != null, .binding = binding orelse .{}, .now = now }); }
        pub fn complete(self: *Self, fence: Fence, value: Result, quiesced: bool, now: u64) Error!void { try self.call(7, .{ .fence = fence, .result = @intFromEnum(value), .flag = quiesced, .now = now }, null); }
        pub fn beginScanout(self: *Self, fence: Fence, now: u64) Error!void { try self.call(8, .{ .fence = fence, .now = now }, null); }
        pub fn scanoutRetireRequested(self: *Self, fence: Fence) Error!bool { return self.result(bool, 9, .{ .fence = fence }); }
        pub fn cancel(self: *Self, fence: Fence, owner: Owner, now: u64) Error!void { try self.call(10, .{ .fence = fence, .owner = owner, .now = now }, null); }
        pub fn expire(self: *Self, now: u64) void { self.call(11, .{ .now = now }, null) catch unreachable; }
        pub fn close(self: *Self, timeline: u64, owner: Owner, now: u64) Error!void { try self.call(12, .{ .timeline = timeline, .owner = owner, .now = now }, null); }
        pub fn stopped(self: *Self, owner: Owner, now: u64) void { self.call(13, .{ .owner = owner, .now = now }, null) catch unreachable; }
        pub fn deviceLost(self: *Self, binding: Binding, now: u64) void { self.call(14, .{ .binding = binding, .now = now }, null) catch unreachable; }
        pub fn drop(self: *Self, fence: Fence, owner: Owner) Error!void { try self.call(15, .{ .fence = fence, .owner = owner }, null); }
        pub fn retainWaiter(self: *Self, fence: Fence) Error!void { try self.call(16, .{ .fence = fence }, null); }
        pub fn releaseWaiter(self: *Self, fence: Fence) Error!void { try self.call(17, .{ .fence = fence }, null); }
        pub fn takeNotification(self: *Self) ?Fence { return self.optional(18, .{}); }
        pub fn published(self: *Self, fence: Fence) Error!void { try self.call(19, .{ .fence = fence }, null); }
        pub fn takeRelease(self: *Self) ?Release { const out = self.result(OptionalRelease, 20, .{}) catch unreachable; return if (out.has) out.value else null; }
        pub fn released(self: *Self, ticket: Release, success: bool) Error!void { try self.call(21, .{ .fence = ticket.fence, .nonce = ticket.nonce, .flag = success }, null); }
        pub fn reapOne(self: *Self) ?Fence { return self.optional(22, .{}); }
        comptime {
            const base = @offsetOf(Self, "timeline_serial");
            if (@offsetOf(Self, "release_serial") != base + 8 or @offsetOf(Self, "next_queue") != base + 16 or @offsetOf(Self, "has_last_owner") != base + 24 or @offsetOf(Self, "last_owner_data") != base + 32 or @offsetOf(Self, "terminal_publications") != base + 56) @compileError("Native queue scalar C layout");
        }
    };
}
comptime {
    if (@sizeOf(Binding) != 24 or @sizeOf(Fence) != 48 or @sizeOf(Config) != 40 or @sizeOf(Status) != 88 or @sizeOf(Release) != 56) @compileError("Native queue transport C layout");
    if (@sizeOf(Queue) != 128 or @alignOf(Queue) != 8 or @offsetOf(Queue, "last_owner_data") != 96 or @offsetOf(Queue, "next_owner_queue") != 120) @compileError("Native queue entry C layout");
    if (@sizeOf(Job) != 144 or @offsetOf(Job, "release_nonce") != 96 or @offsetOf(Job, "dependencies") != 116 or @offsetOf(Job, "dependency_count") != 136) @compileError("Native queue job C layout");
    if (@sizeOf(Scalars) != 64 or @sizeOf(NativeView) != 40 or @sizeOf(NativeArgs) != 216 or @offsetOf(NativeArgs, "dependencies") != 184 or @offsetOf(NativeArgs, "seen") != 200 or @offsetOf(NativeArgs, "pending") != 208) @compileError("Native queue call C layout");
}

const testing = std.testing;
const test_owner = Owner{ .kind = .program, .id = 1, .generation = 2 };

fn fairProducers() !void {
    var state = Store(8, 32){};
    const binding: Binding = .{ .adapter = 9 };
    const owners = [_]Owner{ test_owner, .{ .kind = .program, .id = 2, .generation = 1 }, .{ .kind = .kernel, .id = 3, .generation = 1 } };
    const order = [_]usize{ 0, 0, 1, 2, 0 };
    var timelines: [5]u64 = undefined;
    for (order, 0..) |producer, index| {
        timelines[index] = try state.open(owners[producer], .{ .binding = binding, .capacity = 4 });
        for (0..4) |_| _ = try state.submit(timelines[index], owners[producer], .{ .deadline_ns = 100 }, 0);
    }
    // Kernel owner, app1, app2: one turn per ready producer regardless of
    // app1 opening three queues. The cursor is independent for each adapter.
    const expected = [_]usize{ 2, 0, 1 };
    var previous_app_queue: ?usize = null;
    for (0..12) |turn| {
        const other: Binding = .{ .adapter = 10 };
        if (turn == 0) {
            const timeline = try state.open(owners[2], .{ .binding = other });
            _ = try state.submit(timeline, owners[2], .{ .deadline_ns = 100 }, 0);
        }
        if (turn == 2) {
            const unrelated = state.takeReadyFor(other, 1).?;
            try state.complete(unrelated, .complete, true, 1);
        }
        const selected = state.takeReadyFor(binding, turn + 1).?;
        const queue_index = state.jobs[selected.slot - 1].queue;
        try testing.expect(state.queues[queue_index].owner.eql(owners[expected[turn % 3]]));
        try testing.expect((try state.producer(selected)).eql(owners[expected[turn % 3]]));
        if (expected[turn % 3] == 0) {
            if (previous_app_queue) |prior| try testing.expect(prior != queue_index);
            previous_app_queue = queue_index;
        }
        try state.complete(selected, .complete, true, turn + 1);
    }
}

test "running cancellation wakes once and keeps capacity and DMA until exact late completion" {
    try fairProducers();
    var state = Store(2, 8){};
    const q = try state.open(test_owner, .{ .capacity = 1 });
    const f = try state.submit(q, test_owner, .{ .deadline_ns = 100 }, 0);
    try testing.expectEqualDeep(f, state.takeReady(1).?);
    try state.retainWaiter(f);
    try state.cancel(f, test_owner, 2);
    try testing.expectError(error.Busy, state.submit(q, test_owner, .{ .deadline_ns = 100 }, 2));
    try testing.expect(state.takeRelease() == null);
    try testing.expectEqualDeep(f, state.takeNotification().?);
    try state.published(f);
    try testing.expectError(error.Busy, state.complete(f, .complete, false, 3));
    try state.complete(f, .complete, true, 4);
    try testing.expectEqual(Result.cancelled, (try state.query(f)).result);
    try testing.expectEqual(@as(u64, 2), (try state.query(f)).completed_ns);
    try testing.expect(state.takeNotification() == null);
    try testing.expectError(error.AlreadyCompleted, state.complete(f, .complete, true, 5));
    try state.drop(f, test_owner);
    try state.released(state.takeRelease().?, true);
    try testing.expect(state.reapOne() == null);
    try state.releaseWaiter(f);
    try testing.expectEqualDeep(f, state.reapOne().?);
    try testing.expectError(error.Stale, state.query(f));

    const unplug = try state.submit(q, test_owner, .{ .deadline_ns = 100 }, 6);
    try testing.expectEqualDeep(unplug, state.takeReady(7).?);
    try testing.expectError(error.Busy, state.complete(unplug, .cancelled, false, 8));
    try testing.expect((try state.query(unplug)).device_active and state.takeRelease() == null);
    try state.complete(unplug, .cancelled, true, 9);
    try testing.expect((try state.query(unplug)).result == .cancelled and !(try state.query(unplug)).device_active);
    try testing.expectEqualDeep(unplug, state.takeNotification().?);
    try state.published(unplug);
    try state.drop(unplug, test_owner);
    try state.released(state.takeRelease().?, true);
    try testing.expectEqualDeep(unplug, state.reapOne().?);

    var display = Store(1, 4){};
    const display_queue = try display.open(test_owner, .{ .capacity = 3 });
    const front = try display.submit(display_queue, test_owner, .{ .deadline_ns = 20 }, 10);
    try testing.expectEqualDeep(front, display.takeReady(11).?);
    try display.beginScanout(front, 12);
    try display.beginScanout(front, 12); // Retrying the same activation is harmless.
    display.expire(30); // A static visible image has no running render deadline.
    try testing.expect((try display.query(front)).result == .complete and (try display.query(front)).device_active);
    try testing.expect(display.takeRelease() == null and !try display.scanoutRetireRequested(front));
    const next = try display.submit(display_queue, test_owner, .{ .deadline_ns = 100 }, 31);
    try testing.expectEqualDeep(next, display.takeReady(32).?);
    try display.cancel(front, test_owner, 33);
    try testing.expect(try display.scanoutRetireRequested(front));
    try testing.expectEqual(@as(u64, 12), (try display.query(front)).completed_ns);
    try testing.expectError(error.Busy, display.complete(front, .complete, false, 34));
    try display.complete(front, .complete, true, 35);
    try testing.expectEqualDeep(front, display.takeRelease().?.fence);
    try display.beginScanout(next, 36);
    try display.close(display_queue, test_owner, 37);
    try testing.expect(try display.scanoutRetireRequested(next));
    try testing.expect(display.takeRelease() == null);
    try display.complete(next, .complete, true, 38);
    try testing.expectEqualDeep(next, display.takeRelease().?.fence);
}

test "latest frame replaces only queued work and cannot discard on allocation failure" {
    var state = Store(1, 3){};
    const q = try state.open(test_owner, .{ .policy = .latest_frame, .capacity = 2 });
    const running = try state.submit(q, test_owner, .{ .deadline_ns = 100, .frame_key = 7 }, 0);
    _ = state.takeReady(0);
    const old = try state.submit(q, test_owner, .{ .deadline_ns = 100, .frame_key = 7 }, 0);
    const next = try state.submit(q, test_owner, .{ .deadline_ns = 100, .frame_key = 7 }, 0);
    try testing.expectEqual(Result.dropped, (try state.query(old)).result);
    try testing.expectEqual(Phase.running, (try state.query(running)).phase);
    try testing.expectError(error.Capacity, state.submit(q, test_owner, .{ .deadline_ns = 100, .frame_key = 7 }, 0));
    try testing.expectEqual(Phase.queued, (try state.query(next)).phase);
    try testing.expectEqual(@as(u32, 2), state.queues[0].inflight);
}

test "dependency admission rejects future self and duplicate edges and retains completed parents" {
    var state = Store(2, 8){};
    const a = try state.open(test_owner, .{});
    const b = try state.open(test_owner, .{});
    const parent = try state.submit(a, test_owner, .{ .deadline_ns = 100 }, 0);
    var future = parent;
    future.point += 1;
    try testing.expectError(error.Stale, state.submit(a, test_owner, .{ .deadline_ns = 100, .dependencies = &.{future} }, 0));
    try testing.expectError(error.Invalid, state.submit(b, test_owner, .{ .deadline_ns = 100, .dependencies = &.{ parent, parent } }, 0));
    const child = try state.submit(b, test_owner, .{ .deadline_ns = 100, .dependencies = &.{parent} }, 0);
    try testing.expectEqualDeep(parent, state.takeReady(0).?);
    try testing.expect(state.takeReady(0) == null);
    try state.complete(parent, .complete, true, 1);
    try state.drop(parent, test_owner);
    try state.published(state.takeNotification().?);
    try state.released(state.takeRelease().?, true);
    try testing.expect(state.reapOne() == null);
    try testing.expectEqualDeep(child, state.takeReady(2).?);
    try state.complete(child, .complete, true, 3);
    try state.drop(child, test_owner);
    try state.published(state.takeNotification().?);
    try state.released(state.takeRelease().?, true);
    try testing.expectEqualDeep(child, state.reapOne().?);
    try testing.expectEqualDeep(parent, state.reapOne().?);
}

test "deadline and reset preserve results bindings and physical ownership" {
    var state = Store(2, 8){};
    const binding = Binding{ .adapter = 9, .device_generation = 7, .reset_generation = 3 };
    const q = try state.open(test_owner, .{ .binding = binding });
    const running = try state.submit(q, test_owner, .{ .deadline_ns = 10 }, 0);
    _ = state.takeReady(0);
    const queued = try state.submit(q, test_owner, .{ .deadline_ns = 20 }, 0);
    state.expire(10);
    state.deviceLost(binding, 11);
    try testing.expectEqual(Result.timeout, (try state.query(running)).result);
    try testing.expectEqual(Result.device_lost, (try state.query(queued)).result);
    var stale = running;
    stale.binding.reset_generation += 1;
    try testing.expectError(error.Stale, state.complete(stale, .complete, true, 12));
    try testing.expectError(error.Closed, state.submit(q, test_owner, .{ .deadline_ns = 30 }, 12));
    try testing.expectEqualDeep(queued, state.takeRelease().?.fence);
    try state.complete(running, .complete, true, 13);
    try testing.expectEqual(Result.timeout, (try state.query(running)).result);
    try testing.expectEqual(@as(u64, 2), state.terminal_publications);
    const empty = try state.open(test_owner, .{ .binding = binding });
    state.deviceLost(binding, 14);
    try testing.expectError(error.Stale, state.close(empty, test_owner, 15));
    const next = try state.open(test_owner, .{ .binding = binding });
    try testing.expect(next > empty);
    try testing.expectError(error.Stale, state.close(empty, test_owner, 16));
    try testing.expectError(error.WrongOwner, state.close(next, .{ .kind = .program, .id = 9, .generation = 2 }, 16));
    try state.close(next, test_owner, 16);
}

test "owner death and failed release retain resources without reusing destructor tickets" {
    var state = Store(1, 2){};
    const q = try state.open(test_owner, .{});
    const f = try state.submit(q, test_owner, .{ .deadline_ns = 100 }, 0);
    state.stopped(test_owner, 1);
    const old = state.takeRelease().?;
    try state.released(old, false);
    const retry = state.takeRelease().?;
    try testing.expect(old.nonce != retry.nonce);
    try testing.expectError(error.Stale, state.released(old, true));
    try state.released(retry, true);
    const notification = state.takeNotification().?;
    try testing.expect(state.reapOne() == null);
    try state.published(notification);
    try testing.expectEqualDeep(f, state.reapOne().?);
    const newer = try state.open(test_owner, .{});
    try testing.expect(newer != q);
}

test "explicit dependency paths order hazards but cancellable implicit waits do not" {
    var state = Store(3, 8){};
    const source = try state.open(test_owner, .{});
    const middle = try state.open(test_owner, .{});
    const target = try state.open(test_owner, .{});
    const before = try state.submit(source, test_owner, .{ .deadline_ns = 100 }, 0);
    const explicit = try state.submit(middle, test_owner, .{ .deadline_ns = 100, .dependencies = &.{before} }, 0);
    const implicit = try state.submit(middle, test_owner, .{ .deadline_ns = 100 }, 0);
    try testing.expect(try state.orders(target, &.{explicit}, before));
    try testing.expect(try state.orders(target, &.{implicit}, explicit));
    try testing.expect(!try state.orders(target, &.{implicit}, before));
    try state.cancel(explicit, test_owner, 1);
    try testing.expectEqualDeep(before, state.takeReady(1).?);
    try testing.expectEqualDeep(implicit, state.takeReady(1).?);
    try testing.expect(!try state.orders(target, &.{implicit}, before));
}
