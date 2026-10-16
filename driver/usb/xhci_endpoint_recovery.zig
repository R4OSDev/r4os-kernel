comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
// Native USB owner: stateless compatibility facade.
extern fn r4os_rust_usb_endpoint_state(u8) u8;
extern fn r4os_rust_usb_endpoint_prepare(u8) u8;
extern fn r4os_rust_usb_endpoint_refresh(u8) bool;
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

pub const PrepareAction = enum(u8) {
    reconfigure,
    set_dequeue,
    stop,
    reset,
};

pub const context_state_error_completion: u8 = 19;

pub fn stateFromRaw(raw: u8) ?EndpointState {
    const value = r4os_rust_usb_endpoint_state(raw);
    return if (value == 255) null else @enumFromInt(value);
}

pub fn prepareAction(state: EndpointState) PrepareAction {
    return @enumFromInt(r4os_rust_usb_endpoint_prepare(@intFromEnum(state)));
}

pub fn completionNeedsStateRefresh(completion_code: u8) bool {
    return r4os_rust_usb_endpoint_refresh(completion_code);
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
