// Native network helper; caller retains its original owner/storage.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
const Text = @import("../driver/usb/core.zig").Text;
extern fn r4os_rust_dhcp_coordinator_reset(*Coordinator) void;
extern fn r4os_rust_dhcp_coordinator_observe(*Coordinator, *const Input) u8;
extern fn r4os_rust_dhcp_coordinator_start(*Coordinator, u64, u8) bool;
extern fn r4os_rust_dhcp_coordinator_finish(*Coordinator, u64, bool, bool, u64, u64) void;
extern fn r4os_rust_dhcp_coordinator_cancel(*Coordinator, u64, bool) bool;
extern fn r4os_rust_dhcp_coordinator_name(u16, *Text) void;
// Pure DHCP link/retry coordinator shared by the kernel and the deterministic
// 0.59.13 host contract. It owns policy only; packet I/O stays in net/core.zig.

pub const State = enum(u16) {
    disabled = 0,
    static = 1,
    wait_adapter = 2,
    wait_link = 3,
    acquire = 4,
    retry_wait = 5,
    bound = 6,
    renew = 7,
    rebind = 8,
    lease_lost = 9,
};

pub const Action = enum(u8) {
    none,
    acquire,
    renew,
    rebind,
    clear_lease,
};

pub const Operation = enum(u8) {
    acquire,
    renew,
    rebind,
};

pub const Input = extern struct {
    now: u64,
    desired_dhcp: bool,
    adapter_present: bool,
    link_up: bool,
    lease_bound: bool,
    renew_due: bool = false,
    rebind_due: bool = false,
    lease_expired: bool = false,
};

pub const Coordinator = extern struct {
    state: State = .disabled,
    link_generation: u32 = 0,
    operation_generation: u32 = 0,
    transition_tick: u64 = 0,
    next_retry_tick: u64 = 0,
    last_timeout_tick: u64 = 0,
    retry_round: u8 = 0,
    operation_active: bool = false,
    last_link_known: bool = false,
    last_link_up: bool = false,
    recoveries: u64 = 0,
    starts: u64 = 0,
    cancels: u64 = 0,

    pub fn reset(self: *Coordinator) void {
        r4os_rust_dhcp_coordinator_reset(self);
    }

    pub fn observe(self: *Coordinator, input: Input) Action {
        return @enumFromInt(r4os_rust_dhcp_coordinator_observe(self, &input));
    }

    pub fn startOperation(self: *Coordinator, now: u64, operation: Operation) bool {
        return r4os_rust_dhcp_coordinator_start(self, now, @intFromEnum(operation));
    }

    pub fn finishOperation(self: *Coordinator, now: u64, success: bool, timed_out: bool, base_backoff_ticks: u64, max_backoff_ticks: u64) void {
        r4os_rust_dhcp_coordinator_finish(self, now, success, timed_out, base_backoff_ticks, max_backoff_ticks);
    }

    pub fn cancel(self: *Coordinator, now: u64, keep_desired: bool) bool {
        return r4os_rust_dhcp_coordinator_cancel(self, now, keep_desired);
    }

    pub fn stateName(state: State) []const u8 {
        var out: Text = undefined;
        r4os_rust_dhcp_coordinator_name(@intFromEnum(state), &out);
        return out.slice();
    }
};

comptime {
    if (@sizeOf(Input) != 16 or @alignOf(Input) != 8 or @offsetOf(Input, "now") != 0 or @offsetOf(Input, "desired_dhcp") != 8 or @offsetOf(Input, "adapter_present") != 9 or @offsetOf(Input, "link_up") != 10 or @offsetOf(Input, "lease_bound") != 11 or @offsetOf(Input, "renew_due") != 12 or @offsetOf(Input, "rebind_due") != 13 or @offsetOf(Input, "lease_expired") != 14) @compileError("Native network Input layout");
}

comptime {
    if (@sizeOf(Coordinator) != 72 or @alignOf(Coordinator) != 8 or @offsetOf(Coordinator, "state") != 0 or @offsetOf(Coordinator, "link_generation") != 4 or @offsetOf(Coordinator, "operation_generation") != 8 or @offsetOf(Coordinator, "transition_tick") != 16 or @offsetOf(Coordinator, "next_retry_tick") != 24 or @offsetOf(Coordinator, "last_timeout_tick") != 32 or @offsetOf(Coordinator, "retry_round") != 40 or @offsetOf(Coordinator, "operation_active") != 41 or @offsetOf(Coordinator, "last_link_known") != 42 or @offsetOf(Coordinator, "last_link_up") != 43 or @offsetOf(Coordinator, "recoveries") != 48 or @offsetOf(Coordinator, "starts") != 56 or @offsetOf(Coordinator, "cancels") != 64) @compileError("Native network Coordinator layout");
}
