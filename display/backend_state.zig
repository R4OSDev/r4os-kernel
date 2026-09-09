// One display owner, independent of GPU family and the CPU blit optimization.
// The DisplayManager serializes mutations; this module never touches hardware.
pub const State = enum(u32) { unavailable, bootfb, preparing, native, software_native, recovering };
pub const Policy = enum(u32) { automatic, software, software_once };
pub const Reason = enum(u32) { none, no_native_backend, policy_disabled, backend_rejected, prepare_failed, commit_failed, device_lost, restore_failed };
pub const Error = error{ Invalid, Busy, Disabled, Stale, Exhausted };

pub const Snapshot = struct {
    state: State = .unavailable,
    policy: Policy = .automatic,
    reason: Reason = .no_native_backend,
    revision: u64 = 0,
    generation: u64 = 0,
    reset_generation: u64 = 0,
    owner: usize = 0,
    adapter_id: u32 = 0,
    pending_owner: usize = 0,
    pending_adapter_id: u32 = 0,
    pending_generation: u64 = 0,
};

pub const Manager = struct {
    value: Snapshot = .{},
    serial: u64 = 0,

    pub fn initBoot(self: *Manager) void {
        self.* = .{ .serial = 1, .value = .{ .state = .bootfb, .revision = 1, .generation = 1 } };
    }

    pub fn setPolicy(self: *Manager, policy: Policy) Error!void {
        if (self.value.state != .bootfb and self.value.state != .unavailable) return error.Busy;
        if (self.value.owner != 0 or self.value.pending_owner != 0) return error.Busy;
        self.value.policy = policy;
        self.value.reason = if (policy == .automatic) .no_native_backend else .policy_disabled;
        self.changed();
    }

    pub fn reject(self: *Manager, reason: Reason) void {
        if (self.value.state != .bootfb or self.value.policy != .automatic) return;
        self.value.reason = reason;
        self.changed();
    }

    // Preparation is allocation/query only: the old scanout remains valid.
    pub fn begin(self: *Manager, owner: usize, adapter_id: u32) Error!u64 {
        if (owner == 0 or owner > ~@as(u32, 0) or adapter_id == 0) return error.Invalid;
        if (self.value.policy != .automatic) return error.Disabled;
        if (self.value.state != .bootfb) return error.Busy;
        const generation = try self.nextGeneration();
        self.value.state = .preparing;
        self.value.pending_owner = owner;
        self.value.pending_adapter_id = adapter_id;
        self.value.pending_generation = generation;
        self.changed();
        return generation;
    }

    pub fn abort(self: *Manager, owner: usize, generation: u64, reason: Reason) Error!void {
        try self.checkPending(owner, generation);
        self.value.state = .bootfb;
        self.value.reason = reason;
        self.clearPending();
        self.changed();
    }

    pub fn commit(self: *Manager, owner: usize, generation: u64, software: bool) Error!void {
        try self.checkPending(owner, generation);
        self.adoptPending();
        self.value.state = if (software) .software_native else .native;
        self.value.reason = .none;
        self.changed();
    }

    // An uncertain hardware failure retains the candidate owner. It may own
    // DMA/IRQs even though there is no confirmed output; it cannot be unloaded.
    pub fn failCommit(self: *Manager, owner: usize, generation: u64) Error!void {
        try self.checkPending(owner, generation);
        self.adoptPending();
        self.value.state = .unavailable;
        self.value.reason = .commit_failed;
        self.changed();
    }

    pub fn beginRecovery(self: *Manager, owner: usize, generation: u64) Error!u64 {
        try self.checkActive(owner, generation);
        if (self.value.state != .native and self.value.state != .software_native and self.value.state != .unavailable) return error.Busy;
        if (self.value.reset_generation == ~@as(u64, 0)) return error.Exhausted;
        const next = try self.nextGeneration();
        self.value.generation = next;
        self.value.reset_generation += 1;
        self.value.state = .recovering;
        self.value.reason = .device_lost;
        self.changed();
        return next;
    }

    pub fn recoveryFailed(self: *Manager, owner: usize, generation: u64) Error!void {
        try self.checkActive(owner, generation);
        if (self.value.state != .recovering) return error.Busy;
        self.value.state = .unavailable;
        self.value.reason = .restore_failed;
        self.changed();
    }

    // Only after the device owner confirms both hardware quiescence and the
    // original boot scanout. Software metadata alone cannot establish either.
    pub fn restoreBoot(self: *Manager, owner: usize, generation: u64) Error!void {
        try self.checkActive(owner, generation);
        if (self.value.state != .recovering) return error.Busy;
        const next = try self.nextGeneration();
        const policy = self.value.policy;
        const revision = self.value.revision;
        self.value = .{ .state = .bootfb, .policy = policy, .reason = .device_lost, .revision = revision, .generation = next };
        self.changed();
    }

    pub fn checkPending(self: *const Manager, owner: usize, generation: u64) Error!void {
        if (owner == 0 or generation == 0 or self.value.state != .preparing or
            self.value.pending_owner != owner or self.value.pending_generation != generation) return error.Stale;
    }

    pub fn checkActive(self: *const Manager, owner: usize, generation: u64) Error!void {
        if (owner == 0 or generation == 0 or self.value.owner != owner or self.value.generation != generation) return error.Stale;
    }

    pub fn retainsOwner(self: *const Manager, owner: usize) bool {
        return owner != 0 and (self.value.owner == owner or self.value.pending_owner == owner);
    }

    fn adoptPending(self: *Manager) void {
        self.value.owner = self.value.pending_owner;
        self.value.adapter_id = self.value.pending_adapter_id;
        self.value.generation = self.value.pending_generation;
        self.value.reset_generation = 1;
        self.clearPending();
    }

    fn clearPending(self: *Manager) void {
        self.value.pending_owner = 0;
        self.value.pending_adapter_id = 0;
        self.value.pending_generation = 0;
    }

    fn nextGeneration(self: *Manager) Error!u64 {
        if (self.serial == ~@as(u64, 0)) return error.Exhausted;
        self.serial += 1;
        return self.serial;
    }

    fn changed(self: *Manager) void {
        self.value.revision +|= 1;
    }
};

test "failed preparation preserves the boot owner and cannot reuse a generation" {
    const t = @import("std").testing;
    var manager: Manager = .{};
    manager.initBoot();
    const boot_generation = manager.value.generation;
    const first = try manager.begin(91, 4);
    try t.expectEqual(boot_generation, manager.value.generation);
    try t.expect(manager.retainsOwner(91));
    try t.expectError(error.Busy, manager.begin(92, 5));
    try manager.abort(91, first, .prepare_failed);
    const second = try manager.begin(91, 4);
    try t.expect(second > first);
    try t.expectError(error.Stale, manager.commit(91, first, true));
    try t.expectError(error.Stale, manager.abort(92, second, .prepare_failed));
    try manager.abort(91, second, .commit_failed);
    try t.expectEqual(boot_generation, manager.value.generation);
    try t.expect(!manager.retainsOwner(91));
}

test "unproven hardware stop retains the owner across failure and recovery" {
    const t = @import("std").testing;
    var manager: Manager = .{};
    manager.initBoot();
    const candidate = try manager.begin(91, 4);
    try manager.failCommit(91, candidate);
    try t.expectEqual(State.unavailable, manager.value.state);
    try t.expect(manager.retainsOwner(91));
    const recovery = try manager.beginRecovery(91, candidate);
    try t.expectError(error.Stale, manager.restoreBoot(91, candidate));
    try manager.recoveryFailed(91, recovery);
    try t.expect(manager.retainsOwner(91));
    const retry = try manager.beginRecovery(91, recovery);
    try manager.restoreBoot(91, retry);
    try t.expectEqual(State.bootfb, manager.value.state);
    try t.expect(!manager.retainsOwner(91));
    try t.expect(manager.value.generation > retry);
}

test "software policy rejects native admission and exhaustion never wraps" {
    const t = @import("std").testing;
    var manager: Manager = .{};
    manager.initBoot();
    try manager.setPolicy(.software_once);
    try t.expectError(error.Disabled, manager.begin(91, 4));
    try t.expectEqual(Reason.policy_disabled, manager.value.reason);
    try manager.setPolicy(.automatic);
    manager.serial = ~@as(u64, 0);
    try t.expectError(error.Exhausted, manager.begin(91, 4));
    try t.expectEqual(State.bootfb, manager.value.state);
    try t.expect(!manager.retainsOwner(91));
}
