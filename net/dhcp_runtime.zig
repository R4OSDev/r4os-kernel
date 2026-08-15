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

pub const Action = enum {
    none,
    acquire,
    renew,
    rebind,
    clear_lease,
};

pub const Operation = enum {
    acquire,
    renew,
    rebind,
};

pub const Input = struct {
    now: u64,
    desired_dhcp: bool,
    adapter_present: bool,
    link_up: bool,
    lease_bound: bool,
    renew_due: bool = false,
    rebind_due: bool = false,
    lease_expired: bool = false,
};

pub const Coordinator = struct {
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
        self.* = .{};
    }

    pub fn observe(self: *Coordinator, input: Input) Action {
        self.observeLink(input.now, input.adapter_present, input.link_up);

        if (!input.desired_dhcp) {
            self.operation_active = false;
            self.retry_round = 0;
            self.next_retry_tick = 0;
            self.setState(input.now, if (input.adapter_present) .static else .disabled);
            return .none;
        }
        if (!input.adapter_present) {
            self.setState(input.now, .wait_adapter);
            return if (input.lease_bound) .clear_lease else .none;
        }
        if (!input.link_up) {
            self.setState(input.now, .wait_link);
            return if (input.lease_bound) .clear_lease else .none;
        }
        if (self.operation_active) return .none;
        if (input.lease_expired) {
            self.setState(input.now, .lease_lost);
            return .clear_lease;
        }
        if (input.lease_bound) {
            if (input.now < self.next_retry_tick) {
                self.setState(input.now, .retry_wait);
                return .none;
            }
            if (input.rebind_due) return .rebind;
            if (input.renew_due) return .renew;
            self.retry_round = 0;
            self.next_retry_tick = 0;
            self.setState(input.now, .bound);
            return .none;
        }
        if (input.now < self.next_retry_tick) {
            self.setState(input.now, .retry_wait);
            return .none;
        }
        return .acquire;
    }

    pub fn startOperation(self: *Coordinator, now: u64, operation: Operation) bool {
        if (self.operation_active) return false;
        self.operation_active = true;
        self.operation_generation +%= 1;
        if (self.operation_generation == 0) self.operation_generation = 1;
        self.starts +%= 1;
        self.setState(now, switch (operation) {
            .acquire => .acquire,
            .renew => .renew,
            .rebind => .rebind,
        });
        return true;
    }

    pub fn finishOperation(self: *Coordinator, now: u64, success: bool, timed_out: bool, base_backoff_ticks: u64, max_backoff_ticks: u64) void {
        self.operation_active = false;
        if (success) {
            if (self.retry_round != 0) self.recoveries +%= 1;
            self.retry_round = 0;
            self.next_retry_tick = 0;
            self.setState(now, .bound);
            return;
        }
        if (timed_out) self.last_timeout_tick = now;
        if (self.retry_round < 15) self.retry_round += 1;
        var delay = base_backoff_ticks;
        var shift: u8 = 1;
        while (shift < self.retry_round and delay < max_backoff_ticks) : (shift += 1) {
            delay *|= 2;
        }
        if (delay > max_backoff_ticks) delay = max_backoff_ticks;
        self.next_retry_tick = now +| delay;
        self.setState(now, .retry_wait);
    }

    pub fn cancel(self: *Coordinator, now: u64, keep_desired: bool) bool {
        const active = self.operation_active;
        self.operation_active = false;
        if (active) self.cancels +%= 1;
        self.operation_generation +%= 1;
        if (self.operation_generation == 0) self.operation_generation = 1;
        if (keep_desired) {
            if (self.retry_round == 0) self.retry_round = 1;
            self.next_retry_tick = now;
            self.setState(now, .lease_lost);
        } else {
            self.retry_round = 0;
            self.next_retry_tick = 0;
            self.setState(now, .static);
        }
        return active;
    }

    pub fn stateName(state: State) []const u8 {
        return switch (state) {
            .disabled => "disabled",
            .static => "static",
            .wait_adapter => "wait-adapter",
            .wait_link => "wait-link",
            .acquire => "acquire",
            .retry_wait => "retry-wait",
            .bound => "bound",
            .renew => "renew",
            .rebind => "rebind",
            .lease_lost => "lease-lost",
        };
    }

    fn observeLink(self: *Coordinator, now: u64, present: bool, up: bool) void {
        const effective_up = present and up;
        if (!self.last_link_known or self.last_link_up != effective_up) {
            self.last_link_known = true;
            self.last_link_up = effective_up;
            self.link_generation +%= 1;
            if (self.link_generation == 0) self.link_generation = 1;
            self.transition_tick = now;
        }
    }

    fn setState(self: *Coordinator, now: u64, next: State) void {
        if (self.state == next) return;
        self.state = next;
        self.transition_tick = now;
    }
};
