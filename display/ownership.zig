// Display execution is nonblocking and preemptible. Short metadata transitions
// use the established SMP program-state owner; callbacks never run under it.
const kernel = @import("builtin").os.tag == .freestanding;
const state_owner = if (kernel) @import("../memory/owner_locks.zig") else struct {};
const sync = if (kernel) @import("../sched/sync.zig") else struct {};
const lifetime = if (kernel) @import("../sched/task_context.zig") else struct {};
const std = @import("std");
var host_state: if (kernel) void else std.Io.Mutex = if (kernel) {} else .init;
pub const StateToken = if (kernel) state_owner.Token else void;

pub fn enterState() StateToken {
    if (kernel) return state_owner.program_state.acquire();
    host_state.lockUncancelable(std.testing.io);
}

pub fn leaveState(token: StateToken) void {
    if (kernel) {
        state_owner.program_state.release(token);
    } else {
        host_state.unlock(std.testing.io);
    }
}

pub const Execution = struct {
    guard: if (kernel) sync.UnwindGuard else std.Io.Mutex,

    pub fn init(name: []const u8) Execution {
        return .{ .guard = if (kernel) sync.UnwindGuard.init(name) else .init };
    }

    pub fn tryEnter(self: *Execution) bool {
        return self.enter(0);
    }

    pub fn enter(self: *Execution, timeout_ticks: u64) bool {
        if (!kernel) return self.guard.tryLock();
        return r4os_rust_display_execution(0, &self.guard, timeout_ticks);
    }
    pub fn leave(self: *Execution) void {
        if (kernel) { _ = r4os_rust_display_execution(1, &self.guard, 0); }
        else { self.guard.unlock(std.testing.io); }
    }

};

pub const CallToken = if (kernel) lifetime.UnwindToken else struct {
    pub fn admitted(_: @This()) bool {
        return true;
    }
};

pub fn retainCall() CallToken {
    return if (kernel) lifetime.enterUnwind() else .{};
}

pub fn releaseCall(token: CallToken) void {
    if (kernel) _ = lifetime.leaveUnwind(token);
}

extern fn r4os_rust_display_execution(u32, *ExecutionGuard, u64) bool;
const ExecutionGuard = if (kernel) sync.UnwindGuard else anyopaque;
