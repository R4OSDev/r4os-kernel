//! Exact controller source with explicit clock/identity/callback peers only.
use super::*;
use core::cell::Cell;
struct Peer {
    current_calls: Cell<u32>,
    clock_calls: Cell<usize>,
    work_calls: Cell<u32>,
    refuse_at: Option<u32>,
    stamps: [Option<u64>; 2],
    result: i32,
}
impl Peer {
    fn new(begin: Option<u64>, end: Option<u64>) -> Self {
        Self {
            current_calls: Cell::new(0),
            clock_calls: Cell::new(0),
            work_calls: Cell::new(0),
            refuse_at: None,
            stamps: [begin, end],
            result: 0,
        }
    }
}
impl Hardware for Peer {
    fn current(&self) -> bool {
        let call = self.current_calls.get();
        self.current_calls.set(call + 1);
        self.refuse_at != Some(call)
    }
    fn now(&self) -> Option<u64> {
        let call = self.clock_calls.get();
        self.clock_calls.set(call + 1);
        self.stamps.get(call).copied().flatten()
    }
    fn work(&self, iterations: u32) -> i32 {
        assert_eq!(iterations, 64);
        self.work_calls.set(self.work_calls.get() + 1);
        self.result
    }
}
fn value() -> c::CpuAccelerationWork {
    c::CpuAccelerationWork {
        cpu_index: 1,
        epoch: 9,
        capture_sequence: 2,
        iterations: 64,
        work_kind: c::cpu_accel_work_kind_baseline,
        ..Default::default()
    }
}
#[test]
fn accepted_actual_window_has_identity_real_stamps_callback_and_budget_but_no_counter_claim() {
    let peer = Peer::new(Some(10), Some(500));
    let out = run(value(), &peer);
    assert_eq!(
        (
            out.status,
            out.callback_result,
            out.cpu_index,
            out.epoch,
            out.iterations
        ),
        (0, 0, 1, 9, 64)
    );
    assert_eq!((out.begin_ns, out.end_ns, out.granted_mask), (10, 500, 0));
    assert_eq!(
        out.flags,
        c::cpu_accel_work_flag_callback_started
            | c::cpu_accel_work_flag_callback_returned
            | c::cpu_accel_work_flag_identity_valid
            | c::cpu_accel_work_flag_feature_granted
            | c::cpu_accel_work_flag_clock_valid
            | c::cpu_accel_work_flag_within_budget
    );
    assert_eq!(peer.work_calls.get(), 1);
    assert_eq!(out.reserved0, [0; 3]);
}
#[test]
fn every_refused_identity_boundary_is_explicit_even_if_identity_would_recover() {
    for boundary in 0..3 {
        let mut peer = Peer::new(Some(10), Some(11));
        peer.refuse_at = Some(boundary);
        let out = run(value(), &peer);
        assert_eq!(out.status, c::cpu_accel_work_status_identity_changed);
        assert_eq!(out.flags & c::cpu_accel_work_flag_identity_valid, 0);
        assert_eq!(peer.work_calls.get(), u32::from(boundary == 2));
        assert_eq!(out.flags & c::cpu_accel_work_flag_within_budget, 0);
    }
}
#[test]
fn absent_begin_clock_never_runs_work_and_absent_end_never_accepts_it() {
    for missing_end in [false, true] {
        let peer = if missing_end {
            Peer::new(Some(10), None)
        } else {
            Peer::new(None, Some(11))
        };
        let out = run(value(), &peer);
        assert_eq!(out.status, c::cpu_accel_work_status_clock_unavailable);
        assert_eq!(peer.work_calls.get(), u32::from(missing_end));
        assert_eq!(
            out.flags & (c::cpu_accel_work_flag_clock_valid | c::cpu_accel_work_flag_within_budget),
            0
        );
    }
}
#[test]
fn exact_budget_zero_time_regression_and_over_budget_are_distinct() {
    for (end, status, valid, budget) in [
        (10, 0, true, true),
        (10 + c::cpu_accel_max_window_ns, 0, true, true),
        (9, c::cpu_accel_work_status_clock_regressed, false, false),
        (
            11 + c::cpu_accel_max_window_ns,
            c::cpu_accel_work_status_over_budget,
            true,
            false,
        ),
    ] {
        let peer = Peer::new(Some(10), Some(end));
        let out = run(value(), &peer);
        assert_eq!(out.status, status);
        assert_eq!(peer.work_calls.get(), 1);
        assert_eq!(out.flags & c::cpu_accel_work_flag_clock_valid != 0, valid);
        assert_eq!(
            out.flags & c::cpu_accel_work_flag_within_budget != 0,
            budget
        );
    }
}
#[test]
fn callback_abort_keeps_actual_result_and_honest_receipt_without_completed_status() {
    let mut peer = Peer::new(Some(10), Some(11));
    peer.result = -30017;
    let mut input = value();
    input.flags = c::cpu_accel_work_flag_virtual;
    input.granted_mask = c::cpu_accel_feature_sha as u64;
    let out = run(input, &peer);
    assert_eq!(out.status, c::cpu_accel_work_status_callback_aborted);
    assert_eq!(out.callback_result, -30017);
    assert_eq!(
        out.flags & c::cpu_accel_work_flag_virtual,
        c::cpu_accel_work_flag_virtual
    );
    assert_eq!(out.granted_mask, c::cpu_accel_feature_sha as u64);
    assert_eq!(peer.work_calls.get(), 1);
}
