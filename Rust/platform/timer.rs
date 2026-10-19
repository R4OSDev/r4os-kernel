//! Original logical event epoch and BSP deadline backend. IRQ-independent
//! readers consume atomic values; mutations retain BSP/runtime admission.
use super::{clock_math as math, clocksource};
use crate::{
    arch::{hpet, ioapic, lapic, pit, runtime::RuntimeGuard},
    build_config::config,
};
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
pub const NO_DEADLINE: u64 = u64::MAX;
pub const MAX_FINITE: u64 = u64::MAX - 1;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct EventInfo {
    pub requested_hz: u32,
    pub effective_hz: u32,
    pub frequency_numerator: u64,
    pub frequency_denominator: u64,
    pub resolution_ns: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DeadlineStats {
    pub flags: u32,
    pub mode: u32,
    pub armed_deadline: u64,
    pub timer_irqs: u64,
    pub idle_entries: u64,
    pub one_shot_arms: u64,
    pub earlier_reprograms: u64,
    pub cancels: u64,
    pub periodic_resumes: u64,
    pub runtime_fallbacks: u64,
    pub late_irqs: u64,
    pub max_lateness_ticks: u64,
    pub last_lateness_ticks: u64,
    pub last_irq_tick: u64,
}
static BACKEND: AtomicU32 = AtomicU32::new(0);
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static TICK_EPOCH: AtomicU64 = AtomicU64::new(0);
static TICK_ORIGIN: AtomicU64 = AtomicU64::new(0);
static EVENT_EPOCH: AtomicU64 = AtomicU64::new(0);
static EVENT_ORIGIN: AtomicU64 = AtomicU64::new(0);
static HZ: AtomicU32 = AtomicU32::new(pit::DEFAULT_HZ);
static MODERN_ORIGIN: AtomicU64 = AtomicU64::new(0);
static MODERN_HZ: AtomicU64 = AtomicU64::new(0);
static MODERN_MULTIPLIER: AtomicU64 = AtomicU64::new(0);
static MODERN_SHIFT: AtomicU8 = AtomicU8::new(0);
static DEADLINE_ENABLED: AtomicBool = AtomicBool::new(false);
static MODE: AtomicU32 = AtomicU32::new(0);
static ARMED: AtomicU64 = AtomicU64::new(NO_DEADLINE);
static IRQ_COUNT: AtomicU64 = AtomicU64::new(0);
// Private diagnostic count; total IRQ telemetry and the public C record stay unchanged.
static IDLE_IRQ_COUNT: AtomicU64 = AtomicU64::new(0);
static IDLE_ENTRIES: AtomicU64 = AtomicU64::new(0);
static ARMS: AtomicU64 = AtomicU64::new(0);
static EARLIER: AtomicU64 = AtomicU64::new(0);
static CANCELS: AtomicU64 = AtomicU64::new(0);
static RESUMES: AtomicU64 = AtomicU64::new(0);
static FALLBACKS: AtomicU64 = AtomicU64::new(0);
static LATE: AtomicU64 = AtomicU64::new(0);
static MAX_LATENESS: AtomicU64 = AtomicU64::new(0);
static LAST_LATENESS: AtomicU64 = AtomicU64::new(0);
static LAST_IRQ: AtomicU64 = AtomicU64::new(0);

pub fn active_backend() -> u32 {
    BACKEND.load(Ordering::Acquire)
}
pub(crate) fn backend_name() -> &'static [u8] {
    match active_backend() {
        0 => b"PIT",
        1 => b"HPET",
        2 => b"LAPIC timer",
        _ => panic!("Invalid timer backend"),
    }
}
pub fn frequency() -> u32 {
    HZ.load(Ordering::Acquire)
}
fn scale() -> math::FixedScale {
    math::FixedScale {
        multiplier: MODERN_MULTIPLIER.load(Ordering::Acquire),
        shift: MODERN_SHIFT.load(Ordering::Acquire),
    }
}
fn set_scale(value: math::FixedScale) {
    MODERN_SHIFT.store(value.shift, Ordering::Release);
    MODERN_MULTIPLIER.store(value.multiplier, Ordering::Release);
}
fn raw_ticks() -> u64 {
    match active_backend() {
        0 => pit::tick_count(),
        1 | 2 => {
            let scale = scale();
            if MODERN_HZ.load(Ordering::Acquire) == 0 || !scale.valid() {
                return 0;
            }
            let Some(now) = clocksource::now_nanoseconds() else {
                return 0;
            };
            scale.apply(now.wrapping_sub(MODERN_ORIGIN.load(Ordering::Acquire)))
        }
        _ => panic!("Invalid event timer backend"),
    }
}
pub fn tick_count() -> u64 {
    let local = raw_ticks();
    TICK_EPOCH
        .load(Ordering::Acquire)
        .saturating_add(local.wrapping_sub(TICK_ORIGIN.load(Ordering::Acquire)))
}
pub fn event_info() -> EventInfo {
    let rate = match active_backend() {
        0 => math::reduced_rate(u64::from(pit::BASE_HZ), u64::from(pit::divisor())),
        1 | 2 => math::Rate {
            numerator: u64::from(frequency()),
            denominator: 1,
        },
        _ => panic!("Invalid event timer backend"),
    };
    EventInfo {
        requested_hz: frequency(),
        effective_hz: math::rounded_rate(rate.numerator, rate.denominator),
        frequency_numerator: rate.numerator,
        frequency_denominator: rate.denominator,
        resolution_ns: math::resolution_nanoseconds(rate.numerator, rate.denominator),
    }
}
pub fn event_nanoseconds() -> u64 {
    if !INITIALIZED.load(Ordering::Acquire) {
        return 0;
    }
    let local = raw_ticks();
    let elapsed = local.wrapping_sub(EVENT_ORIGIN.load(Ordering::Acquire));
    let rate = event_info();
    EVENT_EPOCH
        .load(Ordering::Acquire)
        .saturating_add(math::rate_to_nanoseconds(
            elapsed,
            rate.frequency_numerator,
            rate.frequency_denominator,
        ))
}
fn rebase() {
    if !INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    let local = raw_ticks();
    TICK_EPOCH.store(
        TICK_EPOCH
            .load(Ordering::Acquire)
            .saturating_add(local.wrapping_sub(TICK_ORIGIN.load(Ordering::Acquire))),
        Ordering::Release,
    );
    let rate = event_info();
    EVENT_EPOCH.store(
        EVENT_EPOCH
            .load(Ordering::Acquire)
            .saturating_add(math::rate_to_nanoseconds(
                local.wrapping_sub(EVENT_ORIGIN.load(Ordering::Acquire)),
                rate.frequency_numerator,
                rate.frequency_denominator,
            )),
        Ordering::Release,
    );
}
fn activate_origins() {
    let local = raw_ticks();
    TICK_ORIGIN.store(local, Ordering::Release);
    EVENT_ORIGIN.store(local, Ordering::Release);
}
fn activate_modern() {
    MODERN_HZ.store(clocksource::snapshot().frequency_hz, Ordering::Release);
    set_scale(math::FixedScale::init_floor(
        u64::from(frequency()),
        math::NANOSECONDS_PER_SECOND,
    ));
    MODERN_ORIGIN.store(
        clocksource::now_nanoseconds().unwrap_or(0),
        Ordering::Release,
    );
}
fn clear_modern_deadlines() {
    MODERN_ORIGIN.store(0, Ordering::Release);
    MODERN_HZ.store(0, Ordering::Release);
    set_scale(math::FixedScale::default());
    DEADLINE_ENABLED.store(false, Ordering::Release);
    MODE.store(0, Ordering::Release);
    ARMED.store(NO_DEADLINE, Ordering::Release);
}
pub fn init_pit(requested: u32) {
    if INITIALIZED.load(Ordering::Acquire) {
        rebase();
    }
    let hz = if requested == 0 {
        pit::DEFAULT_HZ
    } else {
        requested
    };
    HZ.store(hz, Ordering::Release);
    pit::init(hz);
    BACKEND.store(0, Ordering::Release);
    clear_modern_deadlines();
    activate_origins();
    INITIALIZED.store(true, Ordering::Release);
}
fn switch_backend(backend: u32, requested: u32) -> bool {
    let ready = match backend {
        1 => hpet::r4os_rust_hpet_control(6, 0, requested) != 0,
        2 => lapic::r4os_rust_lapic_control(12, 0, requested) != 0,
        _ => panic!("Invalid timer switch"),
    };
    if !ready {
        return false;
    }
    rebase();
    HZ.store(
        if requested == 0 {
            pit::DEFAULT_HZ
        } else {
            requested
        },
        Ordering::Release,
    );
    BACKEND.store(backend, Ordering::Release);
    activate_modern();
    activate_origins();
    true
}
fn fallback_pit() {
    rebase();
    match active_backend() {
        1 => {
            hpet::r4os_rust_hpet_control(11, 0, 0);
        }
        2 => {
            lapic::r4os_rust_lapic_control(17, 0, 0);
        }
        0 => {}
        _ => panic!("Invalid timer fallback"),
    }
    if ioapic::r4os_rust_ioapic_control(0, 0, false) {
        ioapic::r4os_rust_ioapic_control(1, pit::IRQ, false);
    }
    pit::init(frequency());
    BACKEND.store(0, Ordering::Release);
    clear_modern_deadlines();
    activate_origins();
}
fn on_irq() -> u64 {
    if config::ENABLE_COM1_DEBUG && crate::boot::serial::debug_active() {
        crate::boot::serial::r4os_rust_serial_drain();
    }
    match active_backend() {
        0 => pit::on_tick(),
        1 => {
            hpet::r4os_rust_hpet_control(3, 0, 0);
        }
        2 => {
            lapic::r4os_rust_lapic_control(9, 0, 0);
        }
        _ => panic!("Invalid IRQ timer backend"),
    }
    let count = IRQ_COUNT.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    if count & 0x3ff == 0 {
        clocksource::periodic_validate();
        if active_backend() == 2 {
            lapic::revalidate_current_timer();
        }
    }
    let now = tick_count();
    LAST_IRQ.store(now, Ordering::Release);
    if MODE.load(Ordering::Acquire) == 1 {
        IDLE_IRQ_COUNT.fetch_add(1, Ordering::Relaxed);
        LAST_LATENESS.store(0, Ordering::Release);
        let armed = ARMED.load(Ordering::Acquire);
        if armed != NO_DEADLINE && now > armed {
            let lateness = now - armed;
            LAST_LATENESS.store(lateness, Ordering::Release);
            LATE.fetch_add(1, Ordering::Relaxed);
            MAX_LATENESS.fetch_max(lateness, Ordering::Relaxed);
        }
        // Early queued vectors and finite hardware checkpoints do not retire
        // a still-future logical deadline. Idle rearm retains that exact due.
        if armed != NO_DEADLINE && now >= armed {
            ARMED.store(NO_DEADLINE, Ordering::Release);
        }
    }
    now
}
fn capable() -> bool {
    active_backend() != 0 && MODERN_HZ.load(Ordering::Acquire) != 0
}
fn enable_deadlines() -> bool {
    let _owner = RuntimeGuard::acquire();
    let enabled = capable();
    DEADLINE_ENABLED.store(enabled, Ordering::Release);
    MODE.store(0, Ordering::Release);
    ARMED.store(NO_DEADLINE, Ordering::Release);
    enabled
}
fn disarm_backend() {
    match active_backend() {
        1 => {
            hpet::r4os_rust_hpet_control(10, 0, 0);
        }
        2 => {
            lapic::r4os_rust_lapic_control(15, 0, 0);
        }
        0 => {}
        _ => panic!("Invalid one-shot backend"),
    }
}
fn enter_idle(requested: u64) -> bool {
    let _owner = RuntimeGuard::acquire();
    if !DEADLINE_ENABLED.load(Ordering::Acquire) || active_backend() == 0 {
        return false;
    }
    let now = tick_count();
    if MODE.load(Ordering::Acquire) == 0 {
        let ready = match active_backend() {
            1 => hpet::r4os_rust_hpet_control(8, 0, frequency()) != 0,
            2 => lapic::r4os_rust_lapic_control(13, 0, 0) != 0,
            0 => false,
            _ => panic!("Invalid idle backend"),
        };
        if !ready {
            FALLBACKS.fetch_add(1, Ordering::Relaxed);
            fallback_pit();
            return false;
        }
        MODE.store(1, Ordering::Release);
        IDLE_ENTRIES.fetch_add(1, Ordering::Relaxed);
    }
    if requested == NO_DEADLINE {
        disarm_backend();
        ARMED.store(NO_DEADLINE, Ordering::Release);
        return true;
    }
    let armed = ARMED.load(Ordering::Acquire);
    if armed != NO_DEADLINE && requested < armed {
        EARLIER.fetch_add(1, Ordering::Relaxed);
    }
    let delta = math::bounded_deadline_delta(now, requested, MAX_FINITE.saturating_sub(now));
    let programmed = match active_backend() {
        1 => hpet::r4os_rust_hpet_control(9, delta, frequency()),
        2 => {
            // `requested` is an absolute logical tick, not a whole interval
            // starting partway through the current tick. Invert the existing
            // exact event scale before the clock owner maps it to local TSC.
            let raw_tick = requested
                .saturating_sub(TICK_EPOCH.load(Ordering::Acquire))
                .checked_add(TICK_ORIGIN.load(Ordering::Acquire));
            let ns = raw_tick
                .and_then(|tick| scale().invert_ceil(tick))
                .and_then(|ns| MODERN_ORIGIN.load(Ordering::Acquire).checked_add(ns));
            lapic::arm_absolute(delta, ns)
        }
        0 => 0,
        _ => panic!("Invalid one-shot backend"),
    };
    if programmed == 0 {
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
        fallback_pit();
        return false;
    }
    ARMED.store(math::finite_deadline(now, programmed), Ordering::Release);
    ARMS.fetch_add(1, Ordering::Relaxed);
    true
}
fn leave_idle() -> bool {
    // The original frequent periodic path remains a lock-free local branch.
    if MODE.load(Ordering::Acquire) != 1 {
        return active_backend() != 0;
    }
    let _owner = RuntimeGuard::acquire();
    if MODE.load(Ordering::Acquire) != 1 {
        return active_backend() != 0;
    }
    if ARMED.load(Ordering::Acquire) != NO_DEADLINE {
        CANCELS.fetch_add(1, Ordering::Relaxed);
    }
    disarm_backend();
    ARMED.store(NO_DEADLINE, Ordering::Release);
    let ready = match active_backend() {
        1 => hpet::r4os_rust_hpet_control(7, 0, frequency()) != 0,
        2 => lapic::r4os_rust_lapic_control(16, 0, 0) != 0,
        0 => true,
        _ => panic!("Invalid periodic resume backend"),
    };
    if !ready {
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
        fallback_pit();
        return false;
    }
    MODE.store(0, Ordering::Release);
    RESUMES.fetch_add(1, Ordering::Relaxed);
    true
}
pub fn deadline_stats() -> DeadlineStats {
    let _owner = RuntimeGuard::acquire();
    DeadlineStats {
        flags: u32::from(DEADLINE_ENABLED.load(Ordering::Acquire)) | (u32::from(capable()) << 1),
        mode: MODE.load(Ordering::Acquire),
        armed_deadline: ARMED.load(Ordering::Acquire),
        timer_irqs: IRQ_COUNT.load(Ordering::Relaxed),
        idle_entries: IDLE_ENTRIES.load(Ordering::Relaxed),
        one_shot_arms: ARMS.load(Ordering::Relaxed),
        earlier_reprograms: EARLIER.load(Ordering::Relaxed),
        cancels: CANCELS.load(Ordering::Relaxed),
        periodic_resumes: RESUMES.load(Ordering::Relaxed),
        runtime_fallbacks: FALLBACKS.load(Ordering::Relaxed),
        late_irqs: LATE.load(Ordering::Relaxed),
        max_lateness_ticks: MAX_LATENESS.load(Ordering::Relaxed),
        last_lateness_ticks: LAST_LATENESS.load(Ordering::Acquire),
        last_irq_tick: LAST_IRQ.load(Ordering::Acquire),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_timer_control(operation: u8, a: u64, b: u64) -> u64 {
    match operation {
        0 => {
            init_pit(a as u32);
            0
        }
        1 => u64::from(switch_backend(2, a as u32)),
        2 => u64::from(switch_backend(1, a as u32)),
        3 => {
            fallback_pit();
            0
        }
        4 => on_irq(),
        5 => tick_count(),
        6 => math::finite_deadline(a, b),
        7 => math::finite_deadline(tick_count(), a),
        8 => {
            if b == NO_DEADLINE {
                NO_DEADLINE
            } else {
                b.saturating_sub(a)
            }
        }
        9 => event_nanoseconds(),
        10 => u64::from(frequency()),
        11 => u64::from(enable_deadlines()),
        12 => u64::from(enter_idle(a)),
        13 => u64::from(leave_idle()),
        14 => u64::from(active_backend()),
        15 => {
            if MODE.load(Ordering::Acquire) == 1 {
                2
            } else {
                u64::from(DEADLINE_ENABLED.load(Ordering::Acquire))
            }
        }
        16 => IDLE_IRQ_COUNT.load(Ordering::Relaxed),
        _ => panic!("Invalid private timer operation"),
    }
}
/// # Safety
/// Caller owns one aligned writable fixed C event-clock output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_timer_event_info(output: *mut EventInfo) {
    unsafe { output.write(event_info()) };
}
/// # Safety
/// Caller owns one aligned writable fixed C deadline snapshot until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_timer_deadlines(output: *mut DeadlineStats) {
    unsafe { output.write(deadline_stats()) };
}
const _: () = {
    assert!(core::mem::size_of::<EventInfo>() == 32);
    assert!(core::mem::offset_of!(EventInfo, frequency_numerator) == 8);
    assert!(core::mem::size_of::<DeadlineStats>() == 104);
    assert!(core::mem::offset_of!(DeadlineStats, last_irq_tick) == 96);
};
