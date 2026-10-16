//! Private fixed C projection. The enclosing display owner remains the caller.
comptime { if (@import("builtin").is_test) _ = @import("rust_owner_test"); }
// One display owner, independent of GPU family and the CPU blit optimization.
// The DisplayManager serializes mutations; this module never touches hardware.
pub const State = enum(u32) { unavailable, bootfb, preparing, native, software_native, recovering };
pub const Policy = enum(u32) { automatic, software, software_once };
pub const Reason = enum(u32) { none, no_native_backend, policy_disabled, backend_rejected, prepare_failed, commit_failed, device_lost, restore_failed };
pub const Error = error{ Invalid, Busy, Disabled, Stale, Exhausted };

pub const Snapshot = extern struct {
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

const Restored = extern struct { owner: usize = 0, adapter: u32 = 0, generation: u64 = 0 };
const Args = extern struct { owner: usize = 0, generation: u64 = 0, adapter: u32 = 0, reason: u32 = 0, flag: bool = false };
extern fn r4os_rust_display_backend(u32, *const Manager, *const Args, *u64) i32;
pub const Manager = extern struct {
    value: Snapshot = .{}, serial: u64 = 0, recovering_preparation: bool = false,
    has_restored_boot: bool = false, restored_boot: Restored = .{},
    fn invoke(self: *const Manager, op: u32, args: Args) Error!u64 {
        var value: u64 = undefined;
        switch (r4os_rust_display_backend(op, self, &args, &value)) {
            0 => return value, 1 => return error.Invalid, 3 => return error.Disabled,
            4 => return error.Exhausted, 7 => return error.Stale, 8 => return error.Busy,
            15 => return error.Disabled, else => unreachable,
        }
    }
    pub fn initBoot(self: *Manager) void { _ = self.invoke(0, .{  }) catch unreachable; }
    pub fn setPolicy(self: *Manager, policy: Policy) Error!void { _ = try self.invoke(1, .{ .reason = @intFromEnum(policy) }); }
    pub fn reject(self: *Manager, reason: Reason) void { _ = self.invoke(2, .{ .reason = @intFromEnum(reason) }) catch unreachable; }
    pub fn begin(self: *Manager, owner: usize, adapter_id: u32) Error!u64 { return self.invoke(3, .{ .owner = owner, .adapter = adapter_id }); }
    pub fn abort(self: *Manager, owner: usize, generation: u64, reason: Reason) Error!void { _ = try self.invoke(4, .{ .owner = owner, .generation = generation, .reason = @intFromEnum(reason) }); }
    pub fn commit(self: *Manager, owner: usize, generation: u64, software: bool) Error!void { _ = try self.invoke(5, .{ .owner = owner, .generation = generation, .flag = software }); }
    pub fn failCommit(self: *Manager, owner: usize, generation: u64) Error!void { _ = try self.invoke(6, .{ .owner = owner, .generation = generation }); }
    pub fn beginRecovery(self: *Manager, owner: usize, generation: u64) Error!u64 { return self.invoke(7, .{ .owner = owner, .generation = generation }); }
    pub fn beginDeviceReset(self: *Manager, owner: usize, generation: u64) Error!u64 { return self.invoke(8, .{ .owner = owner, .generation = generation }); }
    pub fn reprepare(self: *Manager, owner: usize, generation: u64, adapter: u32) Error!u64 { return self.invoke(9, .{ .owner = owner, .generation = generation, .adapter = adapter }); }
    pub fn recoveryFailed(self: *Manager, owner: usize, generation: u64) Error!void { _ = try self.invoke(10, .{ .owner = owner, .generation = generation }); }
    pub fn restoreBoot(self: *Manager, owner: usize, generation: u64) Error!void { _ = try self.invoke(11, .{ .owner = owner, .generation = generation }); }
    pub fn finishTerminal(self: *Manager, owner: usize, generation: u64) Error!void { _ = try self.invoke(12, .{ .owner = owner, .generation = generation }); }
    pub fn checkPending(self: *const Manager, owner: usize, generation: u64) Error!void { _ = try self.invoke(13, .{ .owner = owner, .generation = generation }); }
    pub fn checkActive(self: *const Manager, owner: usize, generation: u64) Error!void { _ = try self.invoke(14, .{ .owner = owner, .generation = generation }); }
    pub fn modeResult(self: *Manager, owner: usize, generation: u64, confirmed: bool) Error!void { _ = try self.invoke(15, .{ .owner = owner, .generation = generation, .flag = confirmed }); }
    pub fn retainsOwner(self: *const Manager, owner: usize) bool { return (self.invoke(16, .{ .owner = owner }) catch unreachable) != 0; }
    pub fn restoredBootOwner(self: *const Manager, owner: usize, generation: u64, adapter: u32) bool { return (self.invoke(17, .{ .owner = owner, .generation = generation, .adapter = adapter }) catch unreachable) != 0; }
    pub fn beginRestoredBootReset(self: *Manager, owner: usize, generation: u64, adapter: u32) Error!u64 { return self.invoke(18, .{ .owner = owner, .generation = generation, .adapter = adapter }); }
};
comptime {
    if (@sizeOf(Snapshot) != 80 or @alignOf(Snapshot) != 8 or @offsetOf(Snapshot, "revision") != 16 or @offsetOf(Snapshot, "pending_generation") != 72) @compileError("Display snapshot C layout");
    if (@sizeOf(Manager) != 120 or @offsetOf(Manager, "restored_boot") != 96 or @sizeOf(Args) != 32 or @offsetOf(Args, "flag") != 24) @compileError("Display backend C layout");
}

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

    // Also cover startup loss and failed native reconstruction. The saved
    // boot hold must never masquerade as a restored output in either case.
    manager.initBoot();
    const held = try manager.begin(91, 4);
    try t.expectError(error.Stale, manager.beginDeviceReset(92, held));
    const stopped = try manager.beginDeviceReset(91, held);
    try t.expect(stopped > held and manager.value.state == .recovering and manager.retainsOwner(91));
    try t.expectError(error.Stale, manager.reprepare(91, held, 4));
    try t.expectError(error.Stale, manager.reprepare(91, stopped, 5));
    const fresh = try manager.reprepare(91, stopped, 4);
    try t.expect(fresh > stopped and manager.value.state == .preparing);
    try manager.abort(91, fresh, .prepare_failed);
    try t.expect(manager.value.state == .unavailable and manager.retainsOwner(91));
    const again = try manager.beginDeviceReset(91, stopped);
    const prepared = try manager.reprepare(91, again, 4);
    const resets = manager.value.reset_generation;
    try t.expectError(error.Stale, manager.commit(91, fresh, true));
    try manager.commit(91, prepared, true);
    try t.expect(manager.value.state == .software_native and manager.value.generation == prepared and
        manager.value.reset_generation == resets and resets > 1);
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
