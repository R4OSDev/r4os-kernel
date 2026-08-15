// Pure xHCI endpoint-recovery policy shared by the real driver and its host
// contract.  A transfer timeout does not imply Halted: a Running endpoint
// must first be stopped, while a protocol stall leaves it Halted and requires
// RESET_ENDPOINT.  SET_TR_DEQUEUE_POINTER is only legal after either path has
// reached Stopped.  Error may update the dequeue pointer directly; only a
// Disabled endpoint requires wider device reconfiguration.

pub const EndpointState = enum(u8) {
    disabled = 0,
    running = 1,
    halted = 2,
    stopped = 3,
    error_state = 4,
};

pub const PrepareAction = enum {
    reconfigure,
    set_dequeue,
    stop,
    reset,
};

pub const context_state_error_completion: u8 = 19;

pub fn stateFromRaw(raw: u8) ?EndpointState {
    return switch (raw) {
        0 => .disabled,
        1 => .running,
        2 => .halted,
        3 => .stopped,
        4 => .error_state,
        else => null,
    };
}

pub fn prepareAction(state: EndpointState) PrepareAction {
    return switch (state) {
        .running => .stop,
        .halted => .reset,
        // xHCI 1.2 section 4.6.10 explicitly permits Set TR Dequeue
        // Pointer in both Stopped and Error.  Error is not the same as
        // Disabled and does not require endpoint reconfiguration.
        .stopped, .error_state => .set_dequeue,
        .disabled => .reconfigure,
    };
}

pub fn completionNeedsStateRefresh(completion_code: u8) bool {
    return completion_code == context_state_error_completion;
}

test "running timeout stops before dequeue update" {
    const testing = @import("std").testing;
    try testing.expectEqual(PrepareAction.stop, prepareAction(.running));
}

test "halted endpoint resets before dequeue update" {
    const testing = @import("std").testing;
    try testing.expectEqual(PrepareAction.reset, prepareAction(.halted));
}

test "stopped endpoint can update dequeue directly" {
    const testing = @import("std").testing;
    try testing.expectEqual(PrepareAction.set_dequeue, prepareAction(.stopped));
}

test "error endpoint can update dequeue directly" {
    const testing = @import("std").testing;
    try testing.expectEqual(PrepareAction.set_dequeue, prepareAction(.error_state));
}

test "disabled endpoint requires reconfiguration" {
    const testing = @import("std").testing;
    try testing.expectEqual(PrepareAction.reconfigure, prepareAction(.disabled));
    try testing.expect(stateFromRaw(5) == null);
}

test "context state completion retries through a fresh DMA state read" {
    const testing = @import("std").testing;
    try testing.expect(completionNeedsStateRefresh(context_state_error_completion));
    try testing.expect(!completionNeedsStateRefresh(1));
    try testing.expect(!completionNeedsStateRefresh(6));
}
