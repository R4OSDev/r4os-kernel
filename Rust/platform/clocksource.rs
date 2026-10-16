//! Shared IRQ-independent hardware clock. Original bounded qualification,
//! CPU correlations, watchdog demotion and monotonically published maximum.
use super::{clock_math as math, cpu};
use crate::arch::{hpet, runtime};
use core::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicU32, AtomicU64, Ordering};
const MAX_CPUS: usize = 32;
const MIN_HZ: u64 = 1_000_000;
const MAX_HZ: u64 = 10_000_000_000;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stamp {
    pub raw: u64,
    pub generation: u32,
    pub kind: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Status {
    pub source: u32,
    pub generation: u32,
    pub frequency_hz: u64,
    pub resolution_ns: u64,
    pub flags: u32,
    pub fallback_reason: u32,
    pub registered_cpu_mask: u64,
    pub max_cpu_skew_ns: u64,
    pub calibration_error_ppm: u64,
}
struct Scale {
    multiplier: AtomicU64,
    shift: AtomicU8,
}
impl Scale {
    const fn new() -> Self {
        Self {
            multiplier: AtomicU64::new(0),
            shift: AtomicU8::new(0),
        }
    }
    fn get(&self) -> math::FixedScale {
        math::FixedScale {
            multiplier: self.multiplier.load(Ordering::Acquire),
            shift: self.shift.load(Ordering::Acquire),
        }
    }
    // BSP configuration precedes AP readers; active source publishes the pair.
    fn set(&self, value: math::FixedScale) {
        self.shift.store(value.shift, Ordering::Release);
        self.multiplier.store(value.multiplier, Ordering::Release);
    }
}
static EARLY_INITIALIZED: AtomicBool = AtomicBool::new(false);
static EARLY_ORIGIN: AtomicU64 = AtomicU64::new(0);
static TSC_PRESENT: AtomicBool = AtomicBool::new(false);
static TSC_INVARIANT: AtomicBool = AtomicBool::new(false);
static TSC_HZ: AtomicU64 = AtomicU64::new(0);
static TSC_SCALE: Scale = Scale::new();
static TSC_CALIBRATED: AtomicBool = AtomicBool::new(false);
static CALIBRATION_ERROR: AtomicU64 = AtomicU64::new(0);
static HPET_CONFIGURED: AtomicBool = AtomicBool::new(false);
static HPET_HZ: AtomicU64 = AtomicU64::new(0);
static HPET_SCALE: Scale = Scale::new();
static HPET_ORIGIN: AtomicU64 = AtomicU64::new(0);
static HPET_EPOCH: AtomicU64 = AtomicU64::new(0);
static REFERENCE_HPET: AtomicU64 = AtomicU64::new(0);
static REFERENCE_TSC: AtomicU64 = AtomicU64::new(0);
static LAST_VALIDATION: AtomicU64 = AtomicU64::new(0);
static CORRECTIONS: [AtomicI64; MAX_CPUS] = [const { AtomicI64::new(0) }; MAX_CPUS];
static REGISTERED: AtomicU64 = AtomicU64::new(0);
static MAX_SKEW: AtomicU64 = AtomicU64::new(0);
static SOURCE: AtomicU32 = AtomicU32::new(0);
static GENERATION: AtomicU32 = AtomicU32::new(0);
static PUBLISHED: AtomicU64 = AtomicU64::new(0);
static FALLBACK: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, Default)]
struct Correlation {
    hpet: u64,
    tsc: u64,
    bracket: u64,
}
struct Calibration {
    ok: bool,
    frequency: u64,
    error: u64,
    reference: Correlation,
    failure: u32,
}
impl Default for Calibration {
    fn default() -> Self {
        Self {
            ok: false,
            frequency: 0,
            error: 0,
            reference: Correlation::default(),
            failure: 4,
        }
    }
}
fn read_tsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("lfence", "rdtsc", out("eax") low, out("edx") high, options(nostack, preserves_flags))
    };
    (u64::from(high) << 32) | u64::from(low)
}
pub fn early_init() {
    if EARLY_INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    EARLY_ORIGIN.store(read_tsc(), Ordering::Release);
    EARLY_INITIALIZED.store(true, Ordering::Release);
}
pub fn active_source() -> u32 {
    SOURCE.load(Ordering::Acquire)
}
pub fn generation() -> u32 {
    GENERATION.load(Ordering::Acquire)
}
pub fn last_published() -> u64 {
    PUBLISHED.load(Ordering::Acquire)
}
pub fn publish(candidate: u64) -> u64 {
    let mut current = PUBLISHED.load(Ordering::Acquire);
    while candidate > current {
        match PUBLISHED.compare_exchange_weak(
            current,
            candidate,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return candidate,
            Err(actual) => current = actual,
        }
    }
    current
}
fn reason(value: u32) {
    FALLBACK.store(value, Ordering::Release);
}
fn activate(source: u32) {
    if active_source() == source {
        return;
    }
    GENERATION.fetch_add(1, Ordering::AcqRel);
    SOURCE.store(source, Ordering::Release);
}
fn install_tsc(frequency: u64) {
    if !(MIN_HZ..=MAX_HZ).contains(&frequency) {
        return;
    }
    let scale = math::FixedScale::init_floor(math::NANOSECONDS_PER_SECOND, frequency);
    if !scale.valid() {
        return;
    }
    TSC_HZ.store(frequency, Ordering::Release);
    TSC_SCALE.set(scale);
}
pub fn configure_cpu() {
    if !EARLY_INITIALIZED.load(Ordering::Acquire) {
        early_init();
    }
    let info = cpu::snapshot();
    TSC_PRESENT.store(info.features.tsc, Ordering::Release);
    TSC_INVARIANT.store(info.features.invariant_tsc, Ordering::Release);
    for correction in &CORRECTIONS {
        correction.store(0, Ordering::Relaxed);
    }
    REGISTERED.store(u64::from(info.features.tsc), Ordering::Release);
    if !info.features.tsc {
        reason(1);
        return;
    }
    let exact = if info.tsc_denominator == 0 || info.tsc_numerator == 0 || info.crystal_hz == 0 {
        0
    } else {
        let frequency = (u128::from(info.crystal_hz) * u128::from(info.tsc_numerator))
            / u128::from(info.tsc_denominator);
        if frequency < u128::from(MIN_HZ) || frequency > u128::from(MAX_HZ) {
            0
        } else {
            frequency as u64
        }
    };
    if exact != 0 {
        install_tsc(exact);
    }
    let flags = 1
        | (u8::from(TSC_SCALE.get().valid()) << 1)
        | (u8::from(info.features.invariant_tsc) << 2)
        | 32;
    if math::choose_hardware_clock(flags) == 1 {
        activate(1);
        reason(0);
    } else if !info.features.invariant_tsc {
        reason(2);
    } else {
        reason(3);
    }
}
fn correlated() -> Correlation {
    let before = read_tsc();
    let hpet = hpet::extended_counter();
    let after = read_tsc();
    if after < before {
        return Correlation {
            hpet,
            tsc: after,
            bracket: u64::MAX,
        };
    }
    let bracket = after - before;
    Correlation {
        hpet,
        tsc: before.wrapping_add(bracket / 2),
        bracket,
    }
}
fn calibrate(exact: u64) -> Calibration {
    if !HPET_CONFIGURED.load(Ordering::Acquire) || !TSC_PRESENT.load(Ordering::Acquire) {
        return Calibration::default();
    }
    let hpet_hz = HPET_HZ.load(Ordering::Acquire);
    let target = (hpet_hz / 200).max(1);
    let mut frequencies = [0; 3];
    let mut best = Correlation::default();
    for (index, frequency) in frequencies.iter_mut().enumerate() {
        let start = correlated();
        let mut polls = 0;
        while polls < 1_000_000 {
            let current = hpet::extended_counter();
            if current < start.hpet {
                return Calibration {
                    failure: 8,
                    ..Calibration::default()
                };
            }
            if current - start.hpet >= target {
                break;
            }
            core::hint::spin_loop();
            polls += 1;
        }
        if polls == 1_000_000 {
            return Calibration {
                failure: 8,
                ..Calibration::default()
            };
        }
        let finish = correlated();
        if finish.hpet <= start.hpet {
            return Calibration {
                failure: 8,
                ..Calibration::default()
            };
        }
        if finish.tsc <= start.tsc || start.bracket == u64::MAX || finish.bracket == u64::MAX {
            return Calibration::default();
        }
        *frequency = math::scale_floor(finish.tsc - start.tsc, hpet_hz, finish.hpet - start.hpet);
        if index == 0 || finish.bracket < best.bracket {
            best = finish;
        }
    }
    if frequencies[0] > frequencies[1] {
        frequencies.swap(0, 1);
    }
    if frequencies[1] > frequencies[2] {
        frequencies.swap(1, 2);
    }
    if frequencies[0] > frequencies[1] {
        frequencies.swap(0, 1);
    }
    let measured = frequencies[1];
    if !(MIN_HZ..=MAX_HZ).contains(&measured) {
        return Calibration::default();
    }
    if math::frequency_error_ppm(frequencies[0], frequencies[2]) > 20_000 {
        return Calibration::default();
    }
    let spread = math::frequency_error_ppm(frequencies[0], frequencies[2]);
    let mut error = spread;
    if exact != 0 {
        let difference = math::frequency_error_ppm(measured, exact);
        error = spread.max(difference);
        if difference > 20_000 {
            return Calibration {
                frequency: measured,
                error,
                failure: 5,
                ..Calibration::default()
            };
        }
    }
    Calibration {
        ok: true,
        frequency: if exact != 0 { exact } else { measured },
        error,
        reference: best,
        failure: 0,
    }
}
pub fn attach_hpet(prior: u64) {
    let status = hpet::snapshot();
    if status.flags & 2 == 0 || status.flags & 4 == 0 || status.frequency_hz == 0 {
        if active_source() == 0 {
            reason(8);
        }
        return;
    }
    HPET_HZ.store(status.frequency_hz, Ordering::Release);
    let scale = math::FixedScale::init_floor(math::NANOSECONDS_PER_SECOND, status.frequency_hz);
    HPET_SCALE.set(scale);
    if !scale.valid() {
        if active_source() == 0 {
            reason(8);
        }
        return;
    }
    HPET_ORIGIN.store(hpet::extended_counter(), Ordering::Release);
    HPET_EPOCH.store(publish(prior), Ordering::Release);
    HPET_CONFIGURED.store(true, Ordering::Release);
    if TSC_PRESENT.load(Ordering::Acquire) {
        let exact = TSC_HZ.load(Ordering::Acquire);
        let calibration = calibrate(exact);
        if calibration.ok {
            if exact == 0 {
                install_tsc(calibration.frequency);
            }
            CALIBRATION_ERROR.store(calibration.error, Ordering::Release);
            TSC_CALIBRATED.store(true, Ordering::Release);
            REFERENCE_HPET.store(calibration.reference.hpet, Ordering::Release);
            REFERENCE_TSC.store(calibration.reference.tsc, Ordering::Release);
            LAST_VALIDATION.store(calibration.reference.tsc, Ordering::Release);
            REGISTERED.store(1, Ordering::Release);
            let flags = 1
                | (u8::from(TSC_SCALE.get().valid()) << 1)
                | (u8::from(TSC_INVARIANT.load(Ordering::Acquire)) << 2)
                | 8
                | 16
                | 32;
            if math::choose_hardware_clock(flags) == 1 {
                activate(1);
                reason(0);
                now_nanoseconds();
                return;
            }
        } else {
            reason(calibration.failure);
            if calibration.failure == 8 {
                HPET_CONFIGURED.store(false, Ordering::Release);
                TSC_CALIBRATED.store(false, Ordering::Release);
                hpet::r4os_rust_hpet_control(12, 0, 0);
                if !TSC_INVARIANT.load(Ordering::Acquire) || !TSC_SCALE.get().valid() {
                    activate(0);
                }
                return;
            }
        }
    } else {
        reason(1);
    }
    // A qualified common HPET also qualifies the BSP when CPUID has no TSC.
    // APs register separately after GS installation; their admission must not
    // leave the already running clock owner absent from the hardware mask.
    REGISTERED.fetch_or(1, Ordering::AcqRel);
    activate(2);
    now_nanoseconds();
}
fn adjusted_tsc() -> u64 {
    let raw = read_tsc();
    let index = runtime::current_cpu() as usize;
    if index >= MAX_CPUS {
        return raw;
    }
    math::apply_signed_correction(raw, CORRECTIONS[index].load(Ordering::Acquire))
}
pub fn register_cpu(index: u32) -> bool {
    if index as usize >= MAX_CPUS {
        return false;
    }
    if index == 0 {
        REGISTERED.fetch_or(1, Ordering::AcqRel);
        return true;
    }
    if active_source() != 1 {
        REGISTERED.fetch_or(1 << index, Ordering::AcqRel);
        return true;
    }
    let scale = TSC_SCALE.get();
    let reference_hpet = REFERENCE_HPET.load(Ordering::Acquire);
    if !HPET_CONFIGURED.load(Ordering::Acquire) || !scale.valid() || reference_hpet == 0 {
        return false;
    }
    let tsc_hz = TSC_HZ.load(Ordering::Acquire);
    let hpet_hz = HPET_HZ.load(Ordering::Acquire);
    let mut best = Correlation::default();
    let mut best_correction = 0;
    let mut minimum = i64::MAX;
    let mut maximum = i64::MIN;
    for sample_index in 0..5 {
        let sample = correlated();
        let expected = REFERENCE_TSC
            .load(Ordering::Acquire)
            .wrapping_add(math::scale_floor(
                sample.hpet.wrapping_sub(reference_hpet),
                tsc_hz,
                hpet_hz,
            ));
        let Some(correction) = math::signed_counter_correction(expected, sample.tsc) else {
            return false;
        };
        minimum = minimum.min(correction);
        maximum = maximum.max(correction);
        if sample_index == 0 || sample.bracket < best.bracket {
            best = sample;
            best_correction = correction;
        }
        core::hint::spin_loop();
    }
    let absolute = scale.apply(best_correction.unsigned_abs());
    let spread = u64::try_from(i128::from(maximum) - i128::from(minimum))
        .expect("Clock correction spread exceeds u64");
    if absolute > 1_000_000_000
        || scale.apply(spread) > 250_000
        || best.bracket > (tsc_hz / 500).max(1)
    {
        return false;
    }
    CORRECTIONS[index as usize].store(best_correction, Ordering::Release);
    MAX_SKEW.fetch_max(absolute, Ordering::AcqRel);
    REGISTERED.fetch_or(1 << index, Ordering::AcqRel);
    true
}
fn fallback_to_hpet(failure: u32) {
    let prior = last_published();
    reason(failure);
    if HPET_CONFIGURED.load(Ordering::Acquire) {
        HPET_ORIGIN.store(hpet::extended_counter(), Ordering::Release);
        HPET_EPOCH.store(publish(prior), Ordering::Release);
        activate(2);
        now_nanoseconds();
    } else {
        activate(0);
    }
}
pub fn finalize_cpu(index: u32) -> bool {
    if index as usize >= MAX_CPUS || unsafe { clock_cpu(0, index) } == 0 {
        return false;
    }
    let bit = 1 << index;
    if REGISTERED.load(Ordering::Acquire) & bit != 0 {
        return true;
    }
    fallback_to_hpet(6);
    if active_source() == 1 {
        return false;
    }
    REGISTERED.fetch_or(bit, Ordering::AcqRel);
    true
}
pub fn periodic_validate() -> bool {
    if runtime::current_cpu() != 0
        || active_source() != 1
        || !HPET_CONFIGURED.load(Ordering::Acquire)
        || !TSC_CALIBRATED.load(Ordering::Acquire)
    {
        return true;
    }
    let current = adjusted_tsc();
    let previous = LAST_VALIDATION.load(Ordering::Acquire);
    if previous != 0 && !math::counter_advanced(previous, current) {
        fallback_to_hpet(7);
        return false;
    }
    LAST_VALIDATION.store(current, Ordering::Release);
    let hpet_now = hpet::extended_counter();
    let tsc_elapsed = TSC_SCALE
        .get()
        .apply(current.wrapping_sub(REFERENCE_TSC.load(Ordering::Acquire)));
    let hpet_elapsed = HPET_SCALE
        .get()
        .apply(hpet_now.wrapping_sub(REFERENCE_HPET.load(Ordering::Acquire)));
    let allowed = 2_000_000u64.saturating_add(math::scale_floor(hpet_elapsed, 5_000, 1_000_000));
    if tsc_elapsed.abs_diff(hpet_elapsed) > allowed {
        fallback_to_hpet(7);
        return false;
    }
    true
}
pub fn capture() -> Stamp {
    match active_source() {
        1 => Stamp {
            raw: adjusted_tsc(),
            kind: 1,
            generation: generation(),
        },
        2 => Stamp {
            raw: hpet::extended_counter(),
            kind: 2,
            generation: generation(),
        },
        0 if EARLY_INITIALIZED.load(Ordering::Acquire) => Stamp {
            raw: read_tsc(),
            kind: 1,
            generation: generation(),
        },
        0 => Stamp::default(),
        _ => panic!("Invalid hardware clock source"),
    }
}
pub fn resolve(stamp: Stamp) -> Option<u64> {
    match stamp.kind {
        0 => None,
        1 => {
            let scale = TSC_SCALE.get();
            let origin = EARLY_ORIGIN.load(Ordering::Acquire);
            (scale.valid() && stamp.raw >= origin).then(|| scale.apply(stamp.raw - origin))
        }
        2 => {
            let origin = HPET_ORIGIN.load(Ordering::Acquire);
            (HPET_CONFIGURED.load(Ordering::Acquire) && stamp.raw >= origin).then(|| {
                HPET_EPOCH
                    .load(Ordering::Acquire)
                    .saturating_add(HPET_SCALE.get().apply(stamp.raw - origin))
            })
        }
        _ => panic!("Invalid hardware clock stamp"),
    }
}
pub fn now_nanoseconds() -> Option<u64> {
    let candidate = match active_source() {
        1 => {
            let raw = adjusted_tsc();
            let origin = EARLY_ORIGIN.load(Ordering::Acquire);
            if raw < origin {
                return Some(last_published());
            }
            TSC_SCALE.get().apply(raw - origin)
        }
        2 => HPET_EPOCH.load(Ordering::Acquire).saturating_add(
            HPET_SCALE
                .get()
                .apply(hpet::extended_counter().wrapping_sub(HPET_ORIGIN.load(Ordering::Acquire))),
        ),
        0 => return None,
        _ => panic!("Invalid hardware clock source"),
    };
    Some(publish(candidate))
}
pub fn snapshot() -> Status {
    let source = active_source();
    let frequency = match source {
        0 => 0,
        1 => TSC_HZ.load(Ordering::Acquire),
        2 => HPET_HZ.load(Ordering::Acquire),
        _ => panic!("Invalid hardware clock source"),
    };
    Status {
        source,
        generation: generation(),
        frequency_hz: frequency,
        resolution_ns: math::resolution_nanoseconds(frequency, 1),
        flags: u32::from(TSC_PRESENT.load(Ordering::Acquire))
            | (u32::from(TSC_INVARIANT.load(Ordering::Acquire)) << 1)
            | (u32::from(TSC_CALIBRATED.load(Ordering::Acquire)) << 2)
            | (u32::from(HPET_CONFIGURED.load(Ordering::Acquire)) << 3),
        registered_cpu_mask: REGISTERED.load(Ordering::Acquire) & unsafe { clock_cpu(1, 0) },
        max_cpu_skew_ns: MAX_SKEW.load(Ordering::Acquire),
        calibration_error_ppm: CALIBRATION_ERROR.load(Ordering::Acquire),
        fallback_reason: FALLBACK.load(Ordering::Acquire),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_clocksource_control(operation: u8, value: u64) -> bool {
    match operation {
        0 => {
            early_init();
            true
        }
        1 => {
            configure_cpu();
            true
        }
        2 => {
            attach_hpet(value);
            true
        }
        3 => register_cpu(value as u32),
        4 => finalize_cpu(value as u32),
        5 => periodic_validate(),
        _ => panic!("Invalid private clocksource operation"),
    }
}
/// # Safety
/// Caller owns an aligned writable u64, written only if the clock is available.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_clocksource_value(
    operation: u8,
    value: u64,
    output: *mut u64,
) -> bool {
    let result = match operation {
        0 => now_nanoseconds(),
        1 => Some(last_published()),
        2 => Some(publish(value)),
        3 => Some(u64::from(active_source())),
        4 => Some(u64::from(generation())),
        _ => panic!("Invalid private clock value operation"),
    };
    let Some(result) = result else {
        return false;
    };
    unsafe { output.write(result) };
    true
}
/// # Safety
/// Caller owns one aligned writable fixed C status output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_clocksource_status(output: *mut Status) {
    unsafe { output.write(snapshot()) };
}
/// # Safety
/// Caller owns one aligned writable fixed C stamp output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_clocksource_capture(output: *mut Stamp) {
    unsafe { output.write(capture()) };
}
/// # Safety
/// Caller supplies an initialized C stamp and aligned writable u64 output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_clocksource_resolve(
    stamp: *const Stamp,
    output: *mut u64,
) -> bool {
    let Some(result) = resolve(unsafe { stamp.read() }) else {
        return false;
    };
    unsafe { output.write(result) };
    true
}
const _: () = {
    assert!(core::mem::size_of::<Stamp>() == 16);
    assert!(core::mem::offset_of!(Stamp, kind) == 12);
    assert!(core::mem::size_of::<Status>() == 56);
    assert!(core::mem::offset_of!(Status, registered_cpu_mask) == 32);
};

unsafe fn clock_cpu(operation: u8, index: u32) -> u64 {
    match operation {
        0 => u64::from(crate::arch::percpu::state(index) == 3),
        1 => crate::arch::percpu::r4os_rust_percpu_control(6, 0, 0, 0),
        _ => panic!("Invalid clock CPU query"),
    }
}

pub(crate) fn fallback_name(reason: u32) -> &'static [u8] {
    match reason {
        0 => b"none",
        1 => b"no-tsc",
        2 => b"non-invariant-without-reference",
        3 => b"frequency-unavailable",
        4 => b"calibration-unstable",
        5 => b"frequency-mismatch",
        6 => b"cpu-skew",
        7 => b"tsc-discontinuity",
        8 => b"hpet-unavailable",
        _ => unreachable!(),
    }
}
