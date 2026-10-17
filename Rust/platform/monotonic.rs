//! Original periodic/hardware monotonic projection and stamp resolution.
//! Public output uses the Contract crate's types/constants without a copy.
use super::{clocksource, timer};
use crate::contract;
pub use clocksource::Stamp;
use core::sync::atomic::{AtomicBool, Ordering};
static PERIODIC_ATTACHED: AtomicBool = AtomicBool::new(false);
static PERIODIC_WAS_ACTIVE: AtomicBool = AtomicBool::new(false);
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub flags: u32,
    pub source: u32,
    pub generation: u32,
    pub event_backend: u32,
    pub instant_ns: u64,
    pub resolution_ns: u64,
    pub source_frequency_hz: u64,
    pub event: timer::EventInfo,
}
fn active_source() -> u32 {
    match clocksource::active_source() {
        1 => 1,
        2 => 2,
        0 => {
            if PERIODIC_ATTACHED.load(Ordering::Acquire) {
                3
            } else {
                0
            }
        }
        _ => panic!("Invalid monotonic hardware source"),
    }
}
fn active_generation() -> u32 {
    let generation = clocksource::generation();
    let periodic = PERIODIC_WAS_ACTIVE.load(Ordering::Acquire);
    if clocksource::active_source() != 0 {
        generation.wrapping_add(u32::from(periodic))
    } else if periodic {
        generation.max(1)
    } else {
        generation
    }
}
pub fn attach_periodic() {
    PERIODIC_ATTACHED.store(true, Ordering::Release);
    if clocksource::active_source() == 0 {
        PERIODIC_WAS_ACTIVE.store(true, Ordering::Release);
        clocksource::publish(timer::event_nanoseconds());
    }
}
pub fn now_nanoseconds() -> Option<u64> {
    if let Some(now) = clocksource::now_nanoseconds() {
        return Some(now);
    }
    if !PERIODIC_ATTACHED.load(Ordering::Acquire) {
        return None;
    }
    Some(clocksource::publish(timer::event_nanoseconds()))
}
pub fn capture() -> Stamp {
    if active_source() == 3 {
        return Stamp {
            raw: now_nanoseconds().unwrap_or(0),
            kind: 3,
            generation: active_generation(),
        };
    }
    let hardware = clocksource::capture();
    Stamp {
        raw: hardware.raw,
        kind: hardware.kind,
        generation: active_generation(),
    }
}
pub fn resolve(stamp: Stamp) -> Option<u64> {
    match stamp.kind {
        0 => None,
        3 => Some(stamp.raw),
        1 | 2 => clocksource::resolve(stamp),
        _ => panic!("Invalid monotonic stamp"),
    }
}
pub fn elapsed(start: Stamp, end: Stamp) -> Option<u64> {
    let start = resolve(start)?;
    let end = resolve(end)?;
    if end < start { None } else { Some(end - start) }
}
pub fn snapshot() -> Snapshot {
    let event = timer::event_info();
    let hardware = clocksource::snapshot();
    let source = active_source();
    let frequency = match source {
        1 | 2 => hardware.frequency_hz,
        3 => u64::from(event.effective_hz),
        0 => 0,
        _ => panic!("Invalid monotonic source"),
    };
    let resolution = match source {
        1 | 2 => hardware.resolution_ns,
        3 => event.resolution_ns,
        0 => 0,
        _ => panic!("Invalid monotonic source"),
    };
    let mut flags = if source != 0 {
        contract::monotonic_clock_flag_valid | contract::monotonic_clock_flag_continuous
    } else {
        0
    };
    match source {
        1 => {
            flags |= contract::monotonic_clock_flag_high_resolution
                | contract::monotonic_clock_flag_irq_independent
                | contract::monotonic_clock_flag_early_origin
                | contract::monotonic_clock_flag_calibrated;
            if hardware.flags & 2 != 0 {
                flags |= contract::monotonic_clock_flag_invariant;
            }
        }
        2 => {
            flags |= contract::monotonic_clock_flag_high_resolution
                | contract::monotonic_clock_flag_irq_independent
                | contract::monotonic_clock_flag_calibrated
        }
        3 => flags |= contract::monotonic_clock_flag_degraded,
        0 => {}
        _ => panic!("Invalid monotonic source"),
    }
    let instant = now_nanoseconds().unwrap_or(0);
    let generation = active_generation();
    let event_backend = timer::active_backend();
    Snapshot {
        flags,
        source,
        generation,
        event_backend,
        instant_ns: instant,
        resolution_ns: resolution,
        source_frequency_hz: frequency,
        event,
    }
}
pub fn attach_hpet() {
    let prior = now_nanoseconds().unwrap_or_else(timer::event_nanoseconds);
    clocksource::attach_hpet(prior);
}
/// # Safety
/// R4SYS/R4D retain the initialized writable Contract output for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_monotonic_clock_api(
    output: *mut contract::MonotonicClockInfo,
) -> i32 {
    let clock = snapshot();
    let value = contract::MonotonicClockInfo {
        flags: clock.flags,
        source: clock.source,
        generation: clock.generation,
        event_backend: clock.event_backend,
        instant_ns: clock.instant_ns,
        frequency_hz: contract::monotonic_clock_frequency_hz,
        resolution_ns: clock.resolution_ns,
        source_frequency_hz: clock.source_frequency_hz,
        event_frequency_numerator: clock.event.frequency_numerator,
        event_frequency_denominator: clock.event.frequency_denominator,
        event_requested_hz: clock.event.requested_hz,
        event_effective_hz: clock.event.effective_hz,
        ..contract::MonotonicClockInfo::default()
    };
    unsafe { output.write(value) };
    i32::from(clock.flags & contract::monotonic_clock_flag_valid != 0)
}
const _: () = {
    assert!(core::mem::size_of::<Snapshot>() == 72);
    assert!(core::mem::offset_of!(Snapshot, event) == 40);
    assert!(core::mem::size_of::<contract::MonotonicClockInfo>() == 80);
    assert!(contract::monotonic_clock_frequency_hz == super::clock_math::NANOSECONDS_PER_SECOND);
    assert!(
        contract::monotonic_clock_source_tsc == 1
            && contract::monotonic_clock_source_hpet == 2
            && contract::monotonic_clock_source_periodic_event == 3
    );
};
