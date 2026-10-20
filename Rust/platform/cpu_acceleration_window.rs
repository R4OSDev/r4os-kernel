//! Pure validity/control flow for one bounded resident callback. No counter,
//! instruction selection, allocation, scheduling or CPU/clock instruction here.
use crate::contract as c;
pub(crate) trait Hardware {
    fn current(&self) -> bool;
    fn now(&self) -> Option<u64>;
    fn work(&self, iterations: u32) -> i32;
}
pub(crate) fn run(mut value: c::CpuAccelerationWork, hw: &impl Hardware) -> c::CpuAccelerationWork {
    if !hw.current() {
        value.status = c::cpu_accel_work_status_identity_changed;
        return value;
    }
    value.flags |= c::cpu_accel_work_flag_identity_valid | c::cpu_accel_work_flag_feature_granted;
    let Some(begin) = hw.now() else {
        value.status = c::cpu_accel_work_status_clock_unavailable;
        return value;
    };
    value.begin_ns = begin;
    // A failed exact boundary must never become Completed merely because
    // identity or the clock happens to recover during final publication.
    if !hw.current() {
        value.flags &= !c::cpu_accel_work_flag_identity_valid;
        value.status = c::cpu_accel_work_status_identity_changed;
        return value;
    }
    value.flags |= c::cpu_accel_work_flag_callback_started;
    value.callback_result = hw.work(value.iterations);
    value.flags |= c::cpu_accel_work_flag_callback_returned;
    if !hw.current() {
        value.flags &= !c::cpu_accel_work_flag_identity_valid;
        value.status = c::cpu_accel_work_status_identity_changed;
        return value;
    }
    let Some(end) = hw.now() else {
        value.status = c::cpu_accel_work_status_clock_unavailable;
        return value;
    };
    value.end_ns = end;
    if end < begin {
        value.status = c::cpu_accel_work_status_clock_regressed;
        return value;
    }
    value.flags |= c::cpu_accel_work_flag_clock_valid;
    if end - begin > c::cpu_accel_max_window_ns {
        value.status = c::cpu_accel_work_status_over_budget;
        return value;
    }
    value.flags |= c::cpu_accel_work_flag_within_budget;
    if value.callback_result != 0 {
        value.status = c::cpu_accel_work_status_callback_aborted;
    }
    value
}

#[cfg(test)]
#[path = "cpu_acceleration_window_tests.rs"]
mod tests;
