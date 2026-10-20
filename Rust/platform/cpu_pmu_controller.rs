//! Actual bounded counter transaction. Hardware/work peers are explicit;
//! no mutable Rust loan or runtime owner spans the pure module callback.
use super::cpu_pmu_decode::{self as d, Plan};
use crate::contract as c;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Busy,
    Fault,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Write {
    NotSubmitted,
    Written,
    Faulted,
}
pub(crate) trait Hardware {
    fn current(&self) -> bool;
    fn now(&self) -> Option<u64>;
    fn read(&self, msr: u32) -> Result<u64, Error>;
    fn write(&self, msr: u32, value: u64) -> Write;
    fn work(&self, iterations: u32) -> i32;
}
pub(crate) struct Outcome {
    pub value: c::CpuPmuResult,
    pub release: bool,
}
struct Transaction {
    value: c::CpuPmuResult,
    touched: bool,
    counter_write: u32,
    counter_restore: u64,
}
impl Transaction {
    fn fail(&mut self, status: u32) {
        if self.value.status == c::cpu_pmu_status_completed {
            self.value.status = status;
        }
    }
    fn read(&mut self, hw: &impl Hardware, msr: u32) -> Option<u64> {
        match hw.read(msr) {
            Ok(value) => Some(value),
            Err(reason) => {
                if reason == Error::Fault {
                    self.value.read_faults += 1;
                }
                self.fail(if reason == Error::Busy {
                    c::cpu_pmu_status_busy
                } else {
                    c::cpu_pmu_status_read_failed
                });
                None
            }
        }
    }
    fn write(&mut self, hw: &impl Hardware, msr: u32, value: u64) -> bool {
        match hw.write(msr, value) {
            Write::NotSubmitted => {
                self.fail(c::cpu_pmu_status_busy);
                false
            }
            result => {
                self.touched = true;
                self.value.write_attempts += 1;
                if result == Write::Faulted {
                    self.fail(c::cpu_pmu_status_write_failed);
                    false
                } else {
                    true
                }
            }
        }
    }
    fn preflight(&mut self, p: Plan, hw: &impl Hardware) -> bool {
        if p.profile == d::INTEL {
            let Some(cap) = self.read(hw, 0x345) else {
                return false;
            };
            if cap & (1 << 13) != 0 {
                self.counter_write = p.counter + 0x400;
                self.value.flags |= c::cpu_pmu_result_flag_full_width;
            }
            for msr in [0x1d9, 0x3f1] {
                let Some(value) = self.read(hw, msr) else {
                    return false;
                };
                if value != 0 {
                    self.fail(c::cpu_pmu_status_busy);
                    return false;
                }
            }
            if p.version >= 4 {
                let Some(value) = self.read(hw, 0x392) else {
                    return false;
                };
                if value != 0 {
                    self.fail(c::cpu_pmu_status_busy);
                    return false;
                }
            }
            let Some(global) = self.read(hw, 0x38f) else {
                return false;
            };
            let Some(status) = self.read(hw, 0x38e) else {
                return false;
            };
            self.value.saved_global = global;
            self.value.saved_status = status;
            // Never erase firmware/debug/foreign state just to make it usable.
            if global != 0 || status != 0 {
                self.fail(c::cpu_pmu_status_busy);
                return false;
            }
        } else {
            // Exact Milan private per-thread bank. No L3/DF/global-V2 scan.
            for index in 0..6 {
                let Some(value) = self.read(hw, 0xc001_0200 + 2 * index) else {
                    return false;
                };
                if value & (d::ENABLE | d::INTERRUPT) != 0 {
                    self.fail(c::cpu_pmu_status_busy);
                    return false;
                }
            }
        }
        let Some(control) = self.read(hw, p.control) else {
            return false;
        };
        let Some(counter) = self.read(hw, p.counter) else {
            return false;
        };
        let allowed = if p.profile == d::INTEL {
            u64::from(u32::MAX)
        } else {
            0xffff
                | (3 << 16)
                | (1 << 18)
                | (1 << 20)
                | (1 << 22)
                | (1 << 23)
                | (255 << 24)
                | (15u64 << 32)
                | (3u64 << 40)
                | if p.counter_index == 2 { 1u64 << 43 } else { 0 }
        };
        if control & (d::ENABLE | d::INTERRUPT) != 0
            || control & !allowed != 0
            || counter & !self.value.counter_mask != 0
        {
            self.fail(c::cpu_pmu_status_busy);
            return false;
        }
        self.value.saved_control = control;
        self.value.saved_counter = counter;
        self.counter_restore =
            if p.profile == d::INTEL && self.value.flags & c::cpu_pmu_result_flag_full_width == 0 {
                let Some(value) = d::legacy_intel_write(counter, p.width) else {
                    self.fail(c::cpu_pmu_status_busy);
                    return false;
                };
                value
            } else {
                counter
            };
        self.value.flags |= c::cpu_pmu_result_flag_snapshot_valid;
        true
    }
    fn prepare(&mut self, p: Plan, hw: &impl Hardware) -> bool {
        let selected = p.select | if p.profile == d::INTEL { d::ENABLE } else { 0 };
        if !self.write(hw, p.control, selected) || !self.write(hw, self.counter_write, 0) {
            return false;
        }
        let control = self.read(hw, p.control);
        let counter = self.read(hw, p.counter);
        if control != Some(selected) || counter != Some(0) {
            self.fail(c::cpu_pmu_status_write_failed);
            return false;
        }
        self.value.raw_before = 0;
        self.value.flags |= c::cpu_pmu_result_flag_raw_before;
        true
    }
    /// Stop/restore exactly owned state. No blind retry after an instruction
    /// failure; retain the claim when actual readbacks cannot prove restoration.
    fn finish(&mut self, p: Plan, hw: &impl Hardware, count: bool) -> bool {
        if !hw.current() {
            self.value.flags &= !c::cpu_pmu_result_flag_identity_valid;
            self.value.status = c::cpu_pmu_status_identity_changed;
            return false;
        }
        if !self.touched {
            self.value.flags |= c::cpu_pmu_result_flag_untouched;
            return true;
        }
        let Some(current) = self.read(hw, p.control) else {
            return false;
        };
        if current != self.value.saved_control
            && current != p.select
            && current != p.select | d::ENABLE
        {
            // A different owner/configuration is not ours to overwrite.
            return false;
        }
        if p.profile == d::INTEL {
            let Some(global) = self.read(hw, 0x38f) else {
                return false;
            };
            if global & 1 != 0 {
                // Preserve any unexpected foreign bits, then quarantine if
                // the final complete original readback cannot be established.
                let _ = self.write(hw, 0x38f, global & !1);
            }
        }
        if current != self.value.saved_control {
            let _ = self.write(hw, p.control, self.value.saved_control);
        }
        let control = self.read(hw, p.control);
        if let Some(value) = control {
            self.value.observed_control = value;
        }
        if control != Some(self.value.saved_control) {
            return false;
        }
        // The original control is disabled. Counter reads now belong to an
        // actually stopped counter even if a separate global stop faulted.
        let counter = self.read(hw, p.counter);
        if count && let Some(value) = counter {
            if value & !self.value.counter_mask == 0 {
                self.value.raw_after = value;
                self.value.flags |= c::cpu_pmu_result_flag_raw_after;
            } else {
                self.fail(c::cpu_pmu_status_read_failed);
            }
        }
        let mut status_ok = true;
        if p.profile == d::INTEL {
            let status = self.read(hw, 0x38e);
            if let Some(status) = status {
                if status & 1 != 0 {
                    self.value.flags |= c::cpu_pmu_result_flag_overflow;
                    // Initial own bit was zero; only this exact bit is W1C.
                    let _ = self.write(hw, 0x390, 1);
                }
            } else {
                status_ok = false;
            }
        }
        if counter != Some(self.value.saved_counter) {
            let _ = self.write(hw, self.counter_write, self.counter_restore);
        }
        let restored_counter = self.read(hw, p.counter);
        if let Some(value) = restored_counter {
            self.value.observed_counter = value;
        }
        let mut ok = restored_counter == Some(self.value.saved_counter);
        if p.profile == d::INTEL {
            let global = self.read(hw, 0x38f);
            let status = self.read(hw, 0x38e);
            if let Some(value) = global {
                self.value.observed_global = value;
            }
            if let Some(value) = status {
                self.value.observed_status = value;
            }
            ok &= global == Some(self.value.saved_global)
                && status_ok
                && status == Some(self.value.saved_status);
        }
        ok
    }
}
pub(crate) fn run(
    p: Plan,
    event: u32,
    cpu: u32,
    epoch: u64,
    generation: u64,
    iterations: u32,
    hw: &impl Hardware,
) -> Outcome {
    let mut t = Transaction {
        value: c::CpuPmuResult {
            cpu_index: cpu,
            epoch,
            generation,
            event_id: event,
            profile: p.profile,
            counter_width: p.width,
            iterations,
            counter_index: p.counter_index,
            event_select: p.select | d::ENABLE,
            counter_mask: d::mask(p.width).unwrap_or(0),
            ..Default::default()
        },
        touched: false,
        counter_write: p.counter,
        counter_restore: 0,
    };
    if !hw.current() || epoch == 0 || generation == 0 {
        t.value.status = c::cpu_pmu_status_identity_changed;
        t.value.flags |= c::cpu_pmu_result_flag_untouched;
        return Outcome {
            value: t.value,
            release: true,
        };
    }
    t.value.flags |= c::cpu_pmu_result_flag_identity_valid;
    if iterations == 0
        || iterations > c::cpu_pmu_max_iterations
        || t.value.counter_mask == 0
        || !matches!(p.profile, d::INTEL | d::MILAN_B1)
        || hw.now().is_none()
    {
        t.value.status = c::cpu_pmu_status_clock_invalid;
        t.value.flags |= c::cpu_pmu_result_flag_untouched;
        return Outcome {
            value: t.value,
            release: true,
        };
    }
    let prepared = t.preflight(p, hw) && t.prepare(p, hw);
    let mut called = false;
    if prepared {
        if let Some(now) = hw.now() {
            t.value.begin_ns = now;
            let enabled = if p.profile == d::INTEL {
                t.write(hw, 0x38f, 1)
            } else {
                t.write(hw, p.control, p.select | d::ENABLE)
            };
            if enabled && hw.current() {
                t.value.flags |= c::cpu_pmu_result_flag_callback_started;
                // No &mut Transaction/Hardware/public output borrow is live
                // over this module call. The native peer owns IF0/CPU lifetime.
                let result = hw.work(iterations);
                t.value.callback_result = result;
                t.value.flags |= c::cpu_pmu_result_flag_callback_returned;
                called = true;
                if result != 0 {
                    t.fail(c::cpu_pmu_status_callback_aborted);
                }
            } else if enabled {
                t.value.flags &= !c::cpu_pmu_result_flag_identity_valid;
                t.fail(c::cpu_pmu_status_identity_changed);
            }
        } else {
            t.fail(c::cpu_pmu_status_clock_invalid);
        }
    }
    let release = t.finish(p, hw, called);
    if release && t.touched {
        t.value.flags |= c::cpu_pmu_result_flag_restored;
    }
    if !release && t.value.status != c::cpu_pmu_status_identity_changed {
        t.value.status = c::cpu_pmu_status_restore_failed;
    }
    if let Some(now) = hw.now() {
        t.value.end_ns = now;
        if called && now >= t.value.begin_ns && now - t.value.begin_ns <= c::cpu_pmu_max_window_ns {
            t.value.flags |= c::cpu_pmu_result_flag_within_budget;
        }
    }
    if called && t.value.flags & c::cpu_pmu_result_flag_within_budget == 0 {
        t.fail(c::cpu_pmu_status_clock_invalid);
    }
    if t.value.flags & c::cpu_pmu_result_flag_overflow != 0 {
        t.fail(c::cpu_pmu_status_overflow);
    }
    let raw = c::cpu_pmu_result_flag_raw_before | c::cpu_pmu_result_flag_raw_after;
    if release && called && t.value.flags & raw == raw {
        t.value.delta_modulo =
            d::delta(t.value.raw_before, t.value.raw_after, p.width).unwrap_or(0);
        if t.value.status == c::cpu_pmu_status_completed
            || t.value.status == c::cpu_pmu_status_overflow
        {
            t.value.flags |= c::cpu_pmu_result_flag_modulo_delta;
        }
        if t.value.status == c::cpu_pmu_status_completed && p.profile == d::INTEL {
            t.value.flags |= c::cpu_pmu_result_flag_absolute_count;
        }
    }
    Outcome {
        value: t.value,
        release,
    }
}

#[cfg(test)]
#[path = "cpu_pmu_controller_tests.rs"]
mod tests;
