// Minimal scheduler-to-foundation bridge for operations that may yield while
// a task-owned invariant is active. This module deliberately does not import
// Task, Scheduler, Heap or VM, so low memory layers can participate without
// creating an import cycle.

pub const UnwindToken = struct {
    counter: ?*u32 = null,
    context_present: bool = false,
    active: bool = false,

    pub fn admitted(self: UnwindToken) bool {
        return !self.context_present or self.active;
    }
};

var current_unwind_counter: ?*u32 = null;

pub fn bind(unwind_counter: *u32) void {
    current_unwind_counter = unwind_counter;
}

pub fn clear() void {
    current_unwind_counter = null;
}

// Boot code has no task context and is therefore admitted without a token.
// Runtime overflow is rejected: proceeding without the count would reopen the
// hard-kill window this bridge exists to close.
pub fn enterUnwind() UnwindToken {
    const counter = current_unwind_counter orelse return .{};
    if (counter.* == 0xFFFF_FFFF) {
        return .{ .counter = counter, .context_present = true };
    }
    counter.* += 1;
    return .{
        .counter = counter,
        .context_present = true,
        .active = true,
    };
}

pub fn leaveUnwind(token: UnwindToken) bool {
    if (!token.active) return !token.context_present;
    const counter = token.counter orelse return false;
    if (counter.* == 0) return false;
    counter.* -= 1;
    return true;
}
