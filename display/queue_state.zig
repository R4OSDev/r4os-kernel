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
pub const Binding = struct {
    adapter: u32 = 0,
    device_generation: u64 = 1,
    reset_generation: u64 = 1,
};
pub const Fence = struct {
    slot: u32 = 0,
    timeline: u64 = 0,
    point: u64 = 0,
    binding: Binding = .{},
};
pub const Config = struct {
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
pub const Status = struct {
    fence: Fence,
    phase: Phase,
    result: Result,
    milestone: Milestone,
    deadline_ns: u64,
    completed_ns: u64,
    device_active: bool,
    resources_held: bool,
};
pub const Release = struct { fence: Fence, nonce: u64 };

pub fn Store(comptime queue_capacity: usize, comptime fence_capacity: usize) type {
    if (queue_capacity == 0 or fence_capacity == 0 or fence_capacity > 65535) @compileError("invalid graphics queue capacity");
    return struct {
        const Self = @This();
        const Queue = struct {
            timeline: u64 = 0,
            owner: Owner = .{ .kind = .kernel, .id = 0, .generation = 0 },
            config: Config = .{},
            next_point: u64 = 1,
            closing: bool = false,
            inflight: u32 = 0,
            jobs: u32 = 0,
        };
        const Job = struct {
            fence: Fence = .{},
            queue: usize = 0,
            phase: Phase = .queued,
            result: Result = .pending,
            deadline_ns: u64 = 0,
            completed_ns: u64 = 0,
            frame_key: u64 = 0,
            client_reference: bool = true,
            active: bool = false,
            resources_held: bool = true,
            release_nonce: u64 = 0,
            notification_pending: bool = false,
            notification_publishing: bool = false,
            waiters: u32 = 0,
            dependents: u32 = 0,
            dependencies: [max_dependencies]u16 = .{0} ** max_dependencies,
            dependency_count: usize = 0,
        };
        queues: [queue_capacity]Queue = .{Queue{}} ** queue_capacity,
        jobs: [fence_capacity]Job = .{Job{}} ** fence_capacity,
        timeline_serial: u64 = 0,
        release_serial: u64 = 0,
        next_queue: usize = 0,
        terminal_publications: u64 = 0,

        pub fn open(self: *Self, owner: Owner, config: Config) Error!u64 {
            if (owner.id == 0 or owner.generation == 0 or config.capacity == 0 or config.capacity > fence_capacity or
                config.binding.device_generation == 0 or config.binding.reset_generation == 0) return error.Invalid;
            if (self.timeline_serial == std.math.maxInt(u64)) return error.Exhausted;
            for (&self.queues) |*queue| if (queue.timeline == 0) {
                self.timeline_serial += 1;
                queue.* = .{ .timeline = self.timeline_serial, .owner = owner, .config = config };
                return queue.timeline;
            };
            return error.Capacity;
        }

        pub fn submit(self: *Self, timeline: u64, owner: Owner, request: Submission, now: u64) Error!Fence {
            const qi = try self.queueFor(timeline, owner);
            const queue = &self.queues[qi];
            if (queue.closing) return error.Closed;
            if (request.deadline_ns <= now or request.deadline_ns == std.math.maxInt(u64) or request.dependencies.len > max_dependencies or
                (queue.config.policy == .latest_frame and request.frame_key == 0)) return error.Invalid;
            if (queue.next_point == std.math.maxInt(u64)) return error.Exhausted;
            var dependencies: [max_dependencies]u16 = .{0} ** max_dependencies;
            for (request.dependencies, 0..) |dependency, index| {
                const slot = try self.fenceIndex(dependency);
                // Dependencies must already exist and are immutable after
                // submit. Future/self edges cannot be admitted, so cycles
                // cannot be constructed, including across queue timelines.
                for (dependencies[0..index]) |previous| if (previous == slot) return error.Invalid;
                if (self.jobs[slot].dependents == std.math.maxInt(u32)) return error.Exhausted;
                dependencies[index] = @intCast(slot);
            }
            var replacement: ?usize = null;
            if (queue.config.policy == .latest_frame) {
                for (&self.jobs, 0..) |job, index| {
                    if (job.fence.slot != 0 and job.queue == qi and job.phase == .queued and job.frame_key == request.frame_key) replacement = index;
                }
            }
            if (replacement == null and queue.inflight >= queue.config.capacity) return error.Busy;
            var free: ?usize = null;
            for (&self.jobs, 0..) |job, index| if (job.fence.slot == 0) {
                free = index;
                break;
            };
            const slot = free orelse return error.Capacity;
            // Do not discard the old frame if admission of the new one fails.
            if (replacement) |old| self.makeTerminal(old, .dropped, now);
            const fence = Fence{ .slot = @intCast(slot + 1), .timeline = timeline, .point = queue.next_point, .binding = queue.config.binding };
            queue.next_point += 1;
            queue.inflight += 1;
            queue.jobs += 1;
            self.jobs[slot] = .{
                .fence = fence,
                .queue = qi,
                .deadline_ns = request.deadline_ns,
                .frame_key = request.frame_key,
                .dependencies = dependencies,
                .dependency_count = request.dependencies.len,
            };
            for (dependencies[0..request.dependencies.len]) |dependency| self.jobs[dependency].dependents += 1;
            return fence;
        }

        pub fn query(self: *Self, fence: Fence) Error!Status {
            const job = &self.jobs[try self.fenceIndex(fence)];
            return .{ .fence = job.fence, .phase = job.phase, .result = job.result, .milestone = self.queues[job.queue].config.milestone, .deadline_ns = job.deadline_ns, .completed_ns = job.completed_ns, .device_active = job.active, .resources_held = job.resources_held };
        }

        pub fn configuration(self: *Self, timeline: u64, owner: Owner) Error!Config {
            const queue = self.queues[try self.queueFor(timeline, owner)];
            if (queue.closing) return error.Closed;
            return queue.config;
        }

        /// A FIFO timeline is implicitly serial. Explicit immutable edges may
        /// cross timelines; walk them without recursion or allocation. A later
        /// point also orders every earlier point on that same timeline.
        /// Do not inherit the dependencies of implicit FIFO predecessors:
        /// cancelling a queued predecessor removes that wait. Cross-timeline
        /// buffer hazards therefore require an explicit dependency path.
        pub fn orders(self: *Self, timeline: u64, dependencies: []const Fence, before: Fence) Error!bool {
            _ = try self.fenceIndex(before);
            if (timeline == before.timeline) return true;
            var seen: [fence_capacity]bool = .{false} ** fence_capacity;
            var pending: [fence_capacity]u16 = undefined;
            var count: usize = 0;
            for (dependencies) |fence| {
                const slot = try self.fenceIndex(fence);
                if (!seen[slot]) {
                    seen[slot] = true;
                    pending[count] = @intCast(slot);
                    count += 1;
                }
            }
            while (count != 0) {
                count -= 1;
                const job = self.jobs[pending[count]];
                if (job.fence.timeline == before.timeline and job.fence.point >= before.point) return true;
                for (job.dependencies[0..job.dependency_count]) |slot| if (!seen[slot]) {
                    seen[slot] = true;
                    pending[count] = slot;
                    count += 1;
                };
            }
            return false;
        }

        pub fn takeReady(self: *Self, now: u64) ?Fence {
            return self.takeReadyFor(null, now);
        }

        pub fn takeReadyFor(self: *Self, binding: ?Binding, now: u64) ?Fence {
            self.expire(now);
            var visited: usize = 0;
            while (visited < queue_capacity) : (visited += 1) {
                const qi = (self.next_queue + visited) % queue_capacity;
                const queue = self.queues[qi];
                if (queue.timeline == 0 or queue.closing) continue;
                if (binding) |wanted| if (!std.meta.eql(wanted, queue.config.binding)) continue;
                var first: ?usize = null;
                var active = false;
                for (&self.jobs, 0..) |job, index| {
                    if (job.fence.slot == 0 or job.queue != qi) continue;
                    if (job.active) active = true;
                    if (job.phase != .queued) continue;
                    if (first == null or job.fence.point < self.jobs[first.?].fence.point) first = index;
                }
                // Cancel and timeout are logical completions. Even then the
                // next point must not touch the previous engine's resources.
                if (active) continue;
                const slot = first orelse continue;
                const job = &self.jobs[slot];
                var blocked = false;
                var failed = false;
                for (job.dependencies[0..job.dependency_count]) |dependency| {
                    const parent = self.jobs[dependency];
                    if (parent.phase != .terminal) blocked = true else if (parent.result != .complete) {
                        failed = true;
                    } else if (parent.active) blocked = true;
                }
                if (failed) {
                    self.makeTerminal(slot, .dependency_failed, now);
                    continue;
                }
                if (blocked) continue; // FIFO does not bypass a blocked head.
                job.phase = .running;
                job.active = true;
                self.next_queue = (qi + 1) % queue_capacity;
                return job.fence;
            }
            return null;
        }

        /// O(1) worker metadata transition. IRQ producers use queue_ingress;
        /// wakeup and resource release are separate, retained worker tickets.
        pub fn complete(self: *Self, fence: Fence, result: Result, quiesced: bool, now: u64) Error!void {
            if (result != .complete and result != .failed) return error.Invalid;
            const slot = try self.fenceIndex(fence);
            const job = &self.jobs[slot];
            if (job.phase == .queued) return error.Invalid;
            if (!job.active) return error.AlreadyCompleted;
            if (!quiesced) return error.Busy;
            job.active = false;
            self.queues[job.queue].inflight -= 1;
            // A late physical acknowledgement retires a cancelled/timed-out
            // operation, but cannot overwrite its already published result.
            if (job.phase != .terminal) self.makeTerminal(slot, result, now);
        }

        pub fn cancel(self: *Self, fence: Fence, owner: Owner, now: u64) Error!void {
            const slot = try self.fenceIndex(fence);
            if (!self.queues[self.jobs[slot].queue].owner.eql(owner)) return error.WrongOwner;
            if (self.jobs[slot].phase == .terminal) return error.AlreadyCompleted;
            self.makeTerminal(slot, .cancelled, now);
        }

        pub fn expire(self: *Self, now: u64) void {
            for (&self.jobs, 0..) |job, index| {
                if (job.fence.slot != 0 and job.phase != .terminal and job.deadline_ns <= now) self.makeTerminal(index, .timeout, now);
            }
        }

        pub fn close(self: *Self, timeline: u64, owner: Owner, now: u64) Error!void {
            const qi = try self.queueFor(timeline, owner);
            self.queues[qi].closing = true;
            for (&self.jobs, 0..) |*job, index| {
                if (job.fence.slot == 0 or job.queue != qi) continue;
                job.client_reference = false;
                if (job.phase != .terminal) self.makeTerminal(index, .cancelled, now);
            }
            if (self.queues[qi].jobs == 0) self.queues[qi] = .{};
        }

        pub fn stopped(self: *Self, owner: Owner, now: u64) void {
            for (&self.queues) |queue| {
                if (queue.timeline != 0 and queue.owner.eql(owner)) self.close(queue.timeline, owner, now) catch unreachable;
            }
        }

        pub fn deviceLost(self: *Self, binding: Binding, now: u64) void {
            for (&self.queues, 0..) |*queue, qi| {
                if (queue.timeline == 0 or !std.meta.eql(queue.config.binding, binding)) continue;
                queue.closing = true;
                for (&self.jobs, 0..) |job, slot| if (job.fence.slot != 0 and job.queue == qi and job.phase != .terminal) self.makeTerminal(slot, .device_lost, now);
                if (queue.jobs == 0) queue.* = .{};
            }
        }

        pub fn drop(self: *Self, fence: Fence, owner: Owner) Error!void {
            const job = &self.jobs[try self.fenceIndex(fence)];
            if (!self.queues[job.queue].owner.eql(owner)) return error.WrongOwner;
            if (!job.client_reference) return error.Stale;
            job.client_reference = false;
        }

        pub fn retainWaiter(self: *Self, fence: Fence) Error!void {
            const job = &self.jobs[try self.fenceIndex(fence)];
            if (job.waiters == std.math.maxInt(u32)) return error.Exhausted;
            job.waiters += 1;
        }
        pub fn releaseWaiter(self: *Self, fence: Fence) Error!void {
            const job = &self.jobs[try self.fenceIndex(fence)];
            if (job.waiters == 0) return error.Invalid;
            job.waiters -= 1;
        }

        pub fn takeNotification(self: *Self) ?Fence {
            for (&self.jobs) |*job| if (job.fence.slot != 0 and job.notification_pending and !job.notification_publishing) {
                job.notification_publishing = true;
                return job.fence;
            };
            return null;
        }
        pub fn published(self: *Self, fence: Fence) Error!void {
            const job = &self.jobs[try self.fenceIndex(fence)];
            if (!job.notification_publishing) return error.Invalid;
            job.notification_publishing = false;
            job.notification_pending = false;
        }

        pub fn takeRelease(self: *Self) ?Release {
            if (self.release_serial == std.math.maxInt(u64)) return null;
            for (&self.jobs) |*job| {
                if (job.fence.slot == 0 or job.phase != .terminal or job.active or !job.resources_held or job.release_nonce != 0) continue;
                self.release_serial += 1;
                job.release_nonce = self.release_serial;
                return .{ .fence = job.fence, .nonce = job.release_nonce };
            }
            return null;
        }
        pub fn released(self: *Self, ticket: Release, success: bool) Error!void {
            const job = &self.jobs[try self.fenceIndex(ticket.fence)];
            if (ticket.nonce == 0 or job.release_nonce != ticket.nonce) return error.Stale;
            job.release_nonce = 0;
            if (success) job.resources_held = false;
        }

        pub fn reapOne(self: *Self) ?Fence {
            for (&self.jobs) |*job| {
                if (job.fence.slot == 0 or job.phase != .terminal or job.active or job.client_reference or job.resources_held or
                    job.notification_pending or job.notification_publishing or job.waiters != 0 or job.dependents != 0) continue;
                const fence = job.fence;
                const qi = job.queue;
                for (job.dependencies[0..job.dependency_count]) |dependency| self.jobs[dependency].dependents -= 1;
                job.* = .{};
                self.queues[qi].jobs -= 1;
                if (self.queues[qi].closing and self.queues[qi].jobs == 0) self.queues[qi] = .{};
                return fence;
            }
            return null;
        }

        fn makeTerminal(self: *Self, slot: usize, result: Result, now: u64) void {
            const job = &self.jobs[slot];
            std.debug.assert(job.phase != .terminal and result != .pending);
            if (job.phase == .queued) self.queues[job.queue].inflight -= 1;
            job.phase = .terminal;
            job.result = result;
            job.completed_ns = now;
            job.notification_pending = true;
            self.terminal_publications +|= 1;
        }
        fn queueFor(self: *Self, timeline: u64, owner: Owner) Error!usize {
            if (timeline == 0) return error.Invalid;
            for (&self.queues, 0..) |queue, index_| {
                if (queue.timeline != timeline) continue;
                if (!queue.owner.eql(owner)) return error.WrongOwner;
                return index_;
            }
            return error.Stale;
        }
        fn fenceIndex(self: *Self, fence: Fence) Error!usize {
            if (fence.slot == 0 or fence.slot > fence_capacity or fence.timeline == 0 or fence.point == 0) return error.Invalid;
            const slot: usize = fence.slot - 1;
            if (!std.meta.eql(self.jobs[slot].fence, fence)) return error.Stale;
            return slot;
        }
    };
}

const testing = std.testing;
const test_owner = Owner{ .kind = .program, .id = 1, .generation = 2 };

test "running cancellation wakes once and keeps capacity and DMA until exact late completion" {
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
