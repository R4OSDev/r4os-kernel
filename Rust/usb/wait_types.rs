//! Fixed private USB wait/clock records; policy is native in wait.rs.
use crate::platform::monotonic::Stamp;
use crate::sched::sync_types::Text;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Deadline {
    pub flags: u64,
    pub start_monotonic: Stamp,
    pub duration_ns: u64,
    pub monotonic_valid: bool,
    pub start_tick: u64,
    pub duration_ticks: u64,
    pub start_hpet: u64,
    pub duration_hpet: u64,
    pub hpet_mask: u64,
    pub start_tsc: u64,
    pub duration_tsc: u64,
    pub fallback_tsc: bool,
}
const _: () = {
    assert!(core::mem::size_of::<Deadline>() == 104);
    assert!(core::mem::align_of::<Deadline>() == 8);
    assert!(core::mem::offset_of!(Deadline, flags) == 0);
    assert!(core::mem::offset_of!(Deadline, start_monotonic) == 8);
    assert!(core::mem::offset_of!(Deadline, duration_ns) == 24);
    assert!(core::mem::offset_of!(Deadline, monotonic_valid) == 32);
    assert!(core::mem::offset_of!(Deadline, start_tick) == 40);
    assert!(core::mem::offset_of!(Deadline, duration_ticks) == 48);
    assert!(core::mem::offset_of!(Deadline, start_hpet) == 56);
    assert!(core::mem::offset_of!(Deadline, duration_hpet) == 64);
    assert!(core::mem::offset_of!(Deadline, hpet_mask) == 72);
    assert!(core::mem::offset_of!(Deadline, start_tsc) == 80);
    assert!(core::mem::offset_of!(Deadline, duration_tsc) == 88);
    assert!(core::mem::offset_of!(Deadline, fallback_tsc) == 96);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WaitResult {
    pub reason: Text,
    pub phase: u8,
    pub requested_ms: u32,
    pub elapsed_ns: u64,
    pub iterations: u64,
    pub blocked_ticks: u64,
    pub retry: u8,
    pub success: bool,
}
const _: () = {
    assert!(core::mem::size_of::<WaitResult>() == 56);
    assert!(core::mem::align_of::<WaitResult>() == 8);
    assert!(core::mem::offset_of!(WaitResult, reason) == 0);
    assert!(core::mem::offset_of!(WaitResult, phase) == 16);
    assert!(core::mem::offset_of!(WaitResult, requested_ms) == 20);
    assert!(core::mem::offset_of!(WaitResult, elapsed_ns) == 24);
    assert!(core::mem::offset_of!(WaitResult, iterations) == 32);
    assert!(core::mem::offset_of!(WaitResult, blocked_ticks) == 40);
    assert!(core::mem::offset_of!(WaitResult, retry) == 48);
    assert!(core::mem::offset_of!(WaitResult, success) == 49);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Metrics {
    pub calls: u64,
    pub successes: u64,
    pub timeouts: u64,
    pub early_calls: u64,
    pub runtime_calls: u64,
    pub requested_ms: u64,
    pub elapsed_ns: u64,
    pub max_elapsed_ns: u64,
    pub iterations: u64,
    pub blocked_ticks: u64,
    pub last_reason: Text,
    pub last_phase: u8,
    pub last_requested_ms: u32,
    pub last_elapsed_ns: u64,
    pub last_iterations: u64,
    pub last_blocked_ticks: u64,
    pub last_retry: u8,
    pub last_success: bool,
}
impl Metrics {
    pub const EMPTY: Self = Self {
        calls: 0,
        successes: 0,
        timeouts: 0,
        early_calls: 0,
        runtime_calls: 0,
        requested_ms: 0,
        elapsed_ns: 0,
        max_elapsed_ns: 0,
        iterations: 0,
        blocked_ticks: 0,
        last_reason: Text::literal(b"none"),
        last_phase: 0,
        last_requested_ms: 0,
        last_elapsed_ns: 0,
        last_iterations: 0,
        last_blocked_ticks: 0,
        last_retry: 0,
        last_success: false,
    };
}
const _: () = {
    assert!(core::mem::size_of::<Metrics>() == 136);
    assert!(core::mem::align_of::<Metrics>() == 8);
    assert!(core::mem::offset_of!(Metrics, calls) == 0);
    assert!(core::mem::offset_of!(Metrics, successes) == 8);
    assert!(core::mem::offset_of!(Metrics, timeouts) == 16);
    assert!(core::mem::offset_of!(Metrics, early_calls) == 24);
    assert!(core::mem::offset_of!(Metrics, runtime_calls) == 32);
    assert!(core::mem::offset_of!(Metrics, requested_ms) == 40);
    assert!(core::mem::offset_of!(Metrics, elapsed_ns) == 48);
    assert!(core::mem::offset_of!(Metrics, max_elapsed_ns) == 56);
    assert!(core::mem::offset_of!(Metrics, iterations) == 64);
    assert!(core::mem::offset_of!(Metrics, blocked_ticks) == 72);
    assert!(core::mem::offset_of!(Metrics, last_reason) == 80);
    assert!(core::mem::offset_of!(Metrics, last_phase) == 96);
    assert!(core::mem::offset_of!(Metrics, last_requested_ms) == 100);
    assert!(core::mem::offset_of!(Metrics, last_elapsed_ns) == 104);
    assert!(core::mem::offset_of!(Metrics, last_iterations) == 112);
    assert!(core::mem::offset_of!(Metrics, last_blocked_ticks) == 120);
    assert!(core::mem::offset_of!(Metrics, last_retry) == 128);
    assert!(core::mem::offset_of!(Metrics, last_success) == 129);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Wait {
    pub deadline: Deadline,
    pub reason: Text,
    pub phase: u8,
    pub requested_ms: u32,
    pub iterations: u64,
    pub blocked_ticks: u64,
    pub retry: u8,
    pub runtime_poll_grace: bool,
}
const _: () = {
    assert!(core::mem::size_of::<Wait>() == 152);
    assert!(core::mem::align_of::<Wait>() == 8);
    assert!(core::mem::offset_of!(Wait, deadline) == 0);
    assert!(core::mem::offset_of!(Wait, reason) == 104);
    assert!(core::mem::offset_of!(Wait, phase) == 120);
    assert!(core::mem::offset_of!(Wait, requested_ms) == 124);
    assert!(core::mem::offset_of!(Wait, iterations) == 128);
    assert!(core::mem::offset_of!(Wait, blocked_ticks) == 136);
    assert!(core::mem::offset_of!(Wait, retry) == 144);
    assert!(core::mem::offset_of!(Wait, runtime_poll_grace) == 145);
};
