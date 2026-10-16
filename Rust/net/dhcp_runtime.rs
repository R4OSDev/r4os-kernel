//! Existing link/lease retry coordinator, with caller-owned state. Packet I/O
//! remains at the network owner and never runs under a coordinator borrow.
use crate::sched::sync_types::Text;
#[repr(u16)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Disabled,
    Static,
    WaitAdapter,
    WaitLink,
    Acquire,
    RetryWait,
    Bound,
    Renew,
    Rebind,
    LeaseLost,
}
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Action {
    None,
    Acquire,
    Renew,
    Rebind,
    ClearLease,
}
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Operation {
    Acquire,
    Renew,
    Rebind,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Input {
    pub now: u64,
    pub desired_dhcp: bool,
    pub adapter_present: bool,
    pub link_up: bool,
    pub lease_bound: bool,
    pub renew_due: bool,
    pub rebind_due: bool,
    pub lease_expired: bool,
}
#[repr(C)]
pub struct Coordinator {
    pub state: State,
    pub link_generation: u32,
    pub operation_generation: u32,
    pub transition_tick: u64,
    pub next_retry_tick: u64,
    pub last_timeout_tick: u64,
    pub retry_round: u8,
    pub operation_active: bool,
    pub last_link_known: bool,
    pub last_link_up: bool,
    pub recoveries: u64,
    pub starts: u64,
    pub cancels: u64,
}
impl Coordinator {
    pub const EMPTY: Self = Self {
        state: State::Disabled,
        link_generation: 0,
        operation_generation: 0,
        transition_tick: 0,
        next_retry_tick: 0,
        last_timeout_tick: 0,
        retry_round: 0,
        operation_active: false,
        last_link_known: false,
        last_link_up: false,
        recoveries: 0,
        starts: 0,
        cancels: 0,
    };
    pub fn reset(&mut self) {
        *self = Self::EMPTY;
    }
    fn set_state(&mut self, now: u64, next: State) {
        if self.state != next {
            self.state = next;
            self.transition_tick = now;
        }
    }
    fn observe_link(&mut self, now: u64, present: bool, up: bool) {
        let up = present && up;
        if !self.last_link_known || self.last_link_up != up {
            self.last_link_known = true;
            self.last_link_up = up;
            self.link_generation = self.link_generation.wrapping_add(1);
            if self.link_generation == 0 {
                self.link_generation = 1;
            }
            self.transition_tick = now;
        }
    }
    pub fn observe(&mut self, input: Input) -> Action {
        self.observe_link(input.now, input.adapter_present, input.link_up);
        if !input.desired_dhcp {
            self.operation_active = false;
            self.retry_round = 0;
            self.next_retry_tick = 0;
            self.set_state(
                input.now,
                if input.adapter_present {
                    State::Static
                } else {
                    State::Disabled
                },
            );
            return Action::None;
        }
        if !input.adapter_present {
            self.set_state(input.now, State::WaitAdapter);
            return if input.lease_bound {
                Action::ClearLease
            } else {
                Action::None
            };
        }
        if !input.link_up {
            self.set_state(input.now, State::WaitLink);
            return if input.lease_bound {
                Action::ClearLease
            } else {
                Action::None
            };
        }
        if self.operation_active {
            return Action::None;
        }
        if input.lease_expired {
            self.set_state(input.now, State::LeaseLost);
            return Action::ClearLease;
        }
        if input.lease_bound {
            if input.now < self.next_retry_tick {
                self.set_state(input.now, State::RetryWait);
                return Action::None;
            }
            if input.rebind_due {
                return Action::Rebind;
            }
            if input.renew_due {
                return Action::Renew;
            }
            self.retry_round = 0;
            self.next_retry_tick = 0;
            self.set_state(input.now, State::Bound);
            return Action::None;
        }
        if input.now < self.next_retry_tick {
            self.set_state(input.now, State::RetryWait);
            return Action::None;
        }
        Action::Acquire
    }
    pub fn start_operation(&mut self, now: u64, operation: Operation) -> bool {
        if self.operation_active {
            return false;
        }
        self.operation_active = true;
        self.operation_generation = self.operation_generation.wrapping_add(1);
        if self.operation_generation == 0 {
            self.operation_generation = 1;
        }
        self.starts = self.starts.wrapping_add(1);
        self.set_state(
            now,
            match operation {
                Operation::Acquire => State::Acquire,
                Operation::Renew => State::Renew,
                Operation::Rebind => State::Rebind,
            },
        );
        true
    }
    pub fn finish_operation(
        &mut self,
        now: u64,
        success: bool,
        timed_out: bool,
        base: u64,
        max: u64,
    ) {
        self.operation_active = false;
        if success {
            if self.retry_round != 0 {
                self.recoveries = self.recoveries.wrapping_add(1);
            }
            self.retry_round = 0;
            self.next_retry_tick = 0;
            self.set_state(now, State::Bound);
            return;
        }
        if timed_out {
            self.last_timeout_tick = now;
        }
        if self.retry_round < 15 {
            self.retry_round += 1;
        }
        let mut delay = base;
        let mut shift = 1;
        while shift < self.retry_round && delay < max {
            delay = delay.saturating_mul(2);
            shift += 1;
        }
        self.next_retry_tick = now.saturating_add(delay.min(max));
        self.set_state(now, State::RetryWait);
    }
    pub fn cancel(&mut self, now: u64, keep_desired: bool) -> bool {
        let active = self.operation_active;
        self.operation_active = false;
        if active {
            self.cancels = self.cancels.wrapping_add(1);
        }
        self.operation_generation = self.operation_generation.wrapping_add(1);
        if self.operation_generation == 0 {
            self.operation_generation = 1;
        }
        if keep_desired {
            if self.retry_round == 0 {
                self.retry_round = 1;
            }
            self.next_retry_tick = now;
            self.set_state(now, State::LeaseLost);
        } else {
            self.retry_round = 0;
            self.next_retry_tick = 0;
            self.set_state(now, State::Static);
        }
        active
    }
}
pub fn state_name(state: State) -> Text {
    Text::literal(match state {
        State::Disabled => b"disabled",
        State::Static => b"static",
        State::WaitAdapter => b"wait-adapter",
        State::WaitLink => b"wait-link",
        State::Acquire => b"acquire",
        State::RetryWait => b"retry-wait",
        State::Bound => b"bound",
        State::Renew => b"renew",
        State::Rebind => b"rebind",
        State::LeaseLost => b"lease-lost",
    })
}
/// # Safety
/// Caller exclusively admits its stable coordinator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_coordinator_reset(state: *mut Coordinator) {
    unsafe { (*state).reset() };
}
/// # Safety
/// Exclusive coordinator, separate readable input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_coordinator_observe(
    state: *mut Coordinator,
    input: *const Input,
) -> u8 {
    unsafe { (*state).observe(*input) as u8 }
}
/// # Safety
/// Exclusive coordinator and valid private operation discriminant.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_coordinator_start(
    state: *mut Coordinator,
    now: u64,
    operation: u8,
) -> bool {
    let operation = match operation {
        0 => Operation::Acquire,
        1 => Operation::Renew,
        2 => Operation::Rebind,
        _ => panic!("DHCP operation"),
    };
    unsafe { (*state).start_operation(now, operation) }
}
/// # Safety
/// Caller exclusively admits its stable coordinator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_coordinator_finish(
    state: *mut Coordinator,
    now: u64,
    success: bool,
    timed_out: bool,
    base: u64,
    max: u64,
) {
    unsafe { (*state).finish_operation(now, success, timed_out, base, max) };
}
/// # Safety
/// Caller exclusively admits its stable coordinator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_coordinator_cancel(
    state: *mut Coordinator,
    now: u64,
    keep: bool,
) -> bool {
    unsafe { (*state).cancel(now, keep) }
}
/// # Safety
/// Exact writable text output and valid private state discriminant.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_coordinator_name(state: u16, out: *mut Text) {
    let state = match state {
        0 => State::Disabled,
        1 => State::Static,
        2 => State::WaitAdapter,
        3 => State::WaitLink,
        4 => State::Acquire,
        5 => State::RetryWait,
        6 => State::Bound,
        7 => State::Renew,
        8 => State::Rebind,
        9 => State::LeaseLost,
        _ => panic!("DHCP state"),
    };
    unsafe { out.write(state_name(state)) };
}

const _: () = {
    assert!(core::mem::size_of::<Input>() == 16);
    assert!(core::mem::align_of::<Input>() == 8);
    assert!(core::mem::offset_of!(Input, now) == 0);
    assert!(core::mem::offset_of!(Input, desired_dhcp) == 8);
    assert!(core::mem::offset_of!(Input, adapter_present) == 9);
    assert!(core::mem::offset_of!(Input, link_up) == 10);
    assert!(core::mem::offset_of!(Input, lease_bound) == 11);
    assert!(core::mem::offset_of!(Input, renew_due) == 12);
    assert!(core::mem::offset_of!(Input, rebind_due) == 13);
    assert!(core::mem::offset_of!(Input, lease_expired) == 14);
};
const _: () = {
    assert!(core::mem::size_of::<Coordinator>() == 72);
    assert!(core::mem::align_of::<Coordinator>() == 8);
    assert!(core::mem::offset_of!(Coordinator, state) == 0);
    assert!(core::mem::offset_of!(Coordinator, link_generation) == 4);
    assert!(core::mem::offset_of!(Coordinator, operation_generation) == 8);
    assert!(core::mem::offset_of!(Coordinator, transition_tick) == 16);
    assert!(core::mem::offset_of!(Coordinator, next_retry_tick) == 24);
    assert!(core::mem::offset_of!(Coordinator, last_timeout_tick) == 32);
    assert!(core::mem::offset_of!(Coordinator, retry_round) == 40);
    assert!(core::mem::offset_of!(Coordinator, operation_active) == 41);
    assert!(core::mem::offset_of!(Coordinator, last_link_known) == 42);
    assert!(core::mem::offset_of!(Coordinator, last_link_up) == 43);
    assert!(core::mem::offset_of!(Coordinator, recoveries) == 48);
    assert!(core::mem::offset_of!(Coordinator, starts) == 56);
    assert!(core::mem::offset_of!(Coordinator, cancels) == 64);
};
