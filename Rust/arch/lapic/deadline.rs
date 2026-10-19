//! The LAPIC owner's CPU-local TSC-deadline mode. All mutations retain local
//! IF=0, never a wait or a foreign callback. Only copied atomic telemetry is
//! read remotely; a timer cannot be borrowed by another CPU or start epoch.
use super::{DIVIDE, INITIAL, LVT, MASKED, PERIODIC, VECTOR, deadline_math as math, read, write};
use crate::{
    arch::{percpu, runtime, safe_msr, smp_policy::MAX_CPUS},
    platform::{clocksource, cpu_topology},
};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
const MSR: u32 = 0x6e0;
const DEADLINE: u32 = 1 << 18;
const COUNT: u32 = 0;
const REPEATING: u32 = 1;
const SINGLE: u32 = 2;
const PAUSED: u32 = 3;
struct Timer {
    epoch: AtomicU64,
    generation: AtomicU32,
    frequency: AtomicU64,
    hz: AtomicU32,
    initial: AtomicU32,
    mode: AtomicU32,
    next: AtomicU64,
    arms: AtomicU64,
    irqs: AtomicU64,
    stale: AtomicU64,
    fallbacks: AtomicU64,
    reason: AtomicU32,
}
impl Timer {
    const fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            generation: AtomicU32::new(0),
            frequency: AtomicU64::new(0),
            hz: AtomicU32::new(0),
            initial: AtomicU32::new(0),
            mode: AtomicU32::new(COUNT),
            next: AtomicU64::new(0),
            arms: AtomicU64::new(0),
            irqs: AtomicU64::new(0),
            stale: AtomicU64::new(0),
            fallbacks: AtomicU64::new(0),
            reason: AtomicU32::new(0),
        }
    }
}
static TIMERS: [Timer; MAX_CPUS] = [const { Timer::new() }; MAX_CPUS];
#[cfg(target_os = "none")]
mod check;
#[cfg(target_os = "none")]
pub(crate) use check::run as boot_check;
struct Local(u64);
impl Local {
    fn acquire() -> Self {
        Self(runtime::save_local())
    }
}
impl Drop for Local {
    fn drop(&mut self) {
        runtime::restore_local(self.0);
    }
}
fn local() -> &'static Timer {
    &TIMERS[percpu::current_index() as usize]
}
fn add(value: &AtomicU64) {
    // This exact CPU with IF=0 is the sole writer, including across INIT/SIPI.
    value.store(
        value.load(Ordering::Relaxed).saturating_add(1),
        Ordering::Relaxed,
    );
}
fn current(t: &Timer) -> bool {
    let cpu = percpu::current_index();
    t.epoch.load(Ordering::Acquire) != 0
        && t.epoch.load(Ordering::Acquire) == cpu_topology::epoch(cpu)
        && clocksource::deadline_clock(cpu).is_some_and(|(frequency, generation)| {
            frequency == t.frequency.load(Ordering::Acquire)
                && generation == t.generation.load(Ordering::Acquire)
        })
}
fn wr(t: &Timer, value: u64) -> bool {
    // The LAPIC owner authorizes this one architectural register and a full
    // finite TSC value (or the architectural disarm value), never an index API.
    unsafe {
        safe_msr::write_exact(
            percpu::current_index(),
            t.epoch.load(Ordering::Acquire),
            MSR,
            value,
        )
    }
    .is_ok()
}
fn mask() {
    write(LVT, VECTOR | DEADLINE | MASKED);
    super::deadline_fence();
}
fn program(t: &Timer, next: u64) -> bool {
    write(LVT, VECTOR | DEADLINE);
    // xAPIC MMIO LVT and the weakly ordered deadline MSR require MFENCE.
    // Use the same explicit boundary also for x2APIC and counter reads.
    super::deadline_fence();
    if !wr(t, next) {
        return false;
    }
    t.next.store(next, Ordering::Release);
    add(&t.arms);
    true
}
fn fallback(t: &Timer, reason: u32) {
    let mode = t.mode.load(Ordering::Acquire);
    let next = t.next.load(Ordering::Acquire);
    mask();
    let _ = wr(t, 0);
    // Changing away from TSC-deadline also architecturally disarms its MSR.
    // Retain the already calibrated count timer: no allocation/calibration in
    // an IRQ, no dependence on the now rejected clock's conversion state.
    let initial = t.initial.load(Ordering::Acquire);
    write(DIVIDE, 3);
    if mode == SINGLE {
        write(LVT, VECTOR | MASKED);
        write(INITIAL, 0);
        if next != 0 {
            let ticks = math::remaining_ticks(
                super::read_tsc(),
                next,
                t.frequency.load(Ordering::Acquire),
                t.hz.load(Ordering::Acquire),
            );
            let maximum = (u64::from(0xffff_fffeu32) / u64::from(initial)).max(1);
            write(LVT, VECTOR);
            write(INITIAL, (ticks.min(maximum) * u64::from(initial)) as u32);
        }
    } else {
        write(DIVIDE, 3);
        write(LVT, VECTOR | PERIODIC);
        write(INITIAL, initial);
    }
    t.next.store(0, Ordering::Release);
    t.mode.store(COUNT, Ordering::Release);
    t.reason.store(reason, Ordering::Release);
    add(&t.fallbacks);
}
fn usable(t: &Timer) -> bool {
    if !matches!(t.mode.load(Ordering::Acquire), REPEATING | SINGLE) {
        return false;
    }
    if t.epoch.load(Ordering::Acquire) != cpu_topology::epoch(percpu::current_index()) {
        // An old epoch cannot program a new CPU using its retained scale or
        // calibration. Keep it masked until the real startup owner rebinds.
        mask();
        let _ = wr(t, 0);
        write(LVT, VECTOR | MASKED);
        write(INITIAL, 0);
        t.next.store(0, Ordering::Release);
        t.mode.store(PAUSED, Ordering::Release);
        t.reason.store(6, Ordering::Release);
        return false;
    }
    if current(t) {
        return true;
    }
    fallback(t, 4);
    false
}
/// Called after the actual CPU topology/clock epoch is admitted, with a
/// calibrated count timer already retained for fallback. BSP/AP share this.
pub(super) fn start(initial: u32, hz: u32) -> bool {
    let _local = Local::acquire();
    let t = local();
    t.mode.store(COUNT, Ordering::Release);
    t.next.store(0, Ordering::Release);
    t.epoch.store(
        cpu_topology::epoch(percpu::current_index()),
        Ordering::Release,
    );
    t.initial.store(initial, Ordering::Release);
    t.hz.store(hz, Ordering::Release);
    t.reason.store(1, Ordering::Release);
    if initial < super::MIN_COUNT || hz == 0 || t.epoch.load(Ordering::Acquire) == 0 {
        return false;
    }
    if !super::local_deadline_capable() {
        return false;
    }
    let Some((frequency, generation)) = clocksource::deadline_clock(percpu::current_index()) else {
        t.reason.store(2, Ordering::Release);
        return false;
    };
    t.frequency.store(frequency, Ordering::Release);
    t.generation.store(generation, Ordering::Release);
    let Some((_, period)) = math::interval(1, frequency, hz) else {
        return false;
    };
    let Some(next) = math::target(super::read_tsc(), period) else {
        return false;
    };
    mask();
    if !wr(t, 0) {
        t.mode.store(REPEATING, Ordering::Release);
        fallback(t, 3);
        return false;
    }
    t.mode.store(REPEATING, Ordering::Release);
    if !program(t, next) || !current(t) {
        fallback(t, 3);
        return false;
    }
    t.reason.store(0, Ordering::Release);
    true
}
/// Copied diagnostic identity when the BSP kept its original HPET/PIT event
/// backend. No LAPIC timer/MSR operation or invented calibration is performed.
pub(super) fn external_backend() {
    let _local = Local::acquire();
    let t = local();
    t.mode.store(COUNT, Ordering::Release);
    t.next.store(0, Ordering::Release);
    t.epoch.store(
        cpu_topology::epoch(percpu::current_index()),
        Ordering::Release,
    );
    t.reason.store(8, Ordering::Release);
}
pub(super) fn one_shot() -> bool {
    let _local = Local::acquire();
    let t = local();
    if !usable(t) {
        return false;
    }
    mask();
    if !wr(t, 0) {
        fallback(t, 3);
        return false;
    }
    t.mode.store(SINGLE, Ordering::Release);
    t.next.store(0, Ordering::Release);
    true
}
pub(super) fn arm(ticks: u64) -> Option<u64> {
    arm_with(ticks, None)
}
pub(super) fn arm_with(ticks: u64, absolute_ns: Option<u64>) -> Option<u64> {
    let _local = Local::acquire();
    let t = local();
    if !usable(t) || t.mode.load(Ordering::Acquire) != SINGLE {
        return None;
    }
    let (programmed, cycles) = math::interval(
        ticks,
        t.frequency.load(Ordering::Acquire),
        t.hz.load(Ordering::Acquire),
    )?;
    let now = super::read_tsc();
    let Some(minimum) = math::target(now, math::MIN_CYCLES) else {
        fallback(t, 5);
        return None;
    };
    let next = if let Some(ns) = absolute_ns.filter(|_| programmed == ticks.max(1)) {
        let Some(raw) = clocksource::local_tsc_deadline(
            percpu::current_index(),
            t.generation.load(Ordering::Acquire),
            ns,
        ) else {
            fallback(t, 4);
            return None;
        };
        raw.max(minimum)
    } else {
        let Some(next) = math::target(now, cycles) else {
            fallback(t, 5);
            return None;
        };
        next
    };
    if !program(t, next) {
        fallback(t, 3);
        return None;
    }
    if !current(t) {
        fallback(t, 4);
        return None;
    }
    Some(programmed)
}
pub(super) fn disarm() -> bool {
    let _local = Local::acquire();
    let t = local();
    if !usable(t) {
        return false;
    }
    mask();
    if !wr(t, 0) {
        fallback(t, 3);
        return false;
    }
    t.next.store(0, Ordering::Release);
    true
}
pub(super) fn resume_periodic() -> bool {
    let _local = Local::acquire();
    let t = local();
    if !usable(t) {
        return false;
    }
    let Some((_, period)) = math::interval(
        1,
        t.frequency.load(Ordering::Acquire),
        t.hz.load(Ordering::Acquire),
    ) else {
        return false;
    };
    let Some(next) = math::target(super::read_tsc(), period) else {
        fallback(t, 5);
        return false;
    };
    t.mode.store(REPEATING, Ordering::Release);
    if !program(t, next) {
        fallback(t, 3);
        return false;
    }
    true
}
/// Admission precedes both BSP logical-timer receipts and AP scheduler ticks.
/// A queued old vector cannot retire an unarmed or still-future new deadline.
pub(crate) fn on_irq() -> bool {
    let _local = Local::acquire();
    let t = local();
    let mode = t.mode.load(Ordering::Acquire);
    if mode == COUNT {
        return true;
    }
    if mode == PAUSED {
        add(&t.stale);
        return false;
    }
    if !usable(t) {
        add(&t.stale);
        return false;
    }
    let now = super::read_tsc();
    let next = t.next.load(Ordering::Acquire);
    if next == 0 {
        add(&t.stale);
        return false;
    }
    if now < next {
        // A virtual/APIC timer can deliver early and has already cleared its
        // single-shot MSR. Re-arm this exact target without postponing it;
        // an old queued vector also cannot consume the still-future deadline.
        add(&t.stale);
        if !program(t, next) {
            fallback(t, 3);
        }
        return false;
    }
    add(&t.irqs);
    if mode == REPEATING {
        let Some((_, period)) = math::interval(
            1,
            t.frequency.load(Ordering::Acquire),
            t.hz.load(Ordering::Acquire),
        ) else {
            fallback(t, 5);
            return true;
        };
        if let Some(next) = math::periodic_next(next, now, period) {
            if !program(t, next) {
                fallback(t, 3);
            }
        } else {
            fallback(t, 5);
        }
    } else {
        t.next.store(0, Ordering::Release);
    }
    true
}
pub(crate) fn revalidate() {
    let _local = Local::acquire();
    let t = local();
    let _ = usable(t);
}
pub(super) struct Paused {
    cpu: u32,
    epoch: u64,
    generation: u32,
    mode: u32,
    next: u64,
}
pub(super) fn pause() -> Result<Option<Paused>, ()> {
    let _local = Local::acquire();
    let t = local();
    if !usable(t) {
        return Ok(None);
    }
    let token = Paused {
        cpu: percpu::current_index(),
        epoch: t.epoch.load(Ordering::Acquire),
        generation: t.generation.load(Ordering::Acquire),
        mode: t.mode.load(Ordering::Acquire),
        next: t.next.load(Ordering::Acquire),
    };
    mask();
    if !wr(t, 0) {
        return Err(());
    }
    let cleared = unsafe { safe_msr::read_exact(token.cpu, token.epoch, MSR) }.map_err(|_| ())?;
    if cleared != 0 || read(LVT) != (VECTOR | DEADLINE | MASKED) {
        return Err(());
    }
    t.next.store(0, Ordering::Release);
    t.mode.store(PAUSED, Ordering::Release);
    Ok(Some(token))
}
pub(super) fn resume(token: Paused) -> bool {
    let _local = Local::acquire();
    let t = local();
    if token.cpu != percpu::current_index()
        || token.epoch != cpu_topology::epoch(token.cpu)
        || token.epoch != t.epoch.load(Ordering::Acquire)
        || t.mode.load(Ordering::Acquire) != PAUSED
    {
        return false;
    }
    t.mode.store(token.mode, Ordering::Release);
    t.next.store(token.next, Ordering::Release);
    if !current(t) || token.generation != t.generation.load(Ordering::Acquire) {
        fallback(t, 4);
        return true;
    }
    if token.mode == SINGLE && token.next == 0 {
        return disarm();
    }
    let now = super::read_tsc();
    let next = if token.next > now {
        Some(token.next)
    } else {
        math::interval(
            1,
            t.frequency.load(Ordering::Acquire),
            t.hz.load(Ordering::Acquire),
        )
        .and_then(|(_, p)| math::target(now, p))
    };
    if let Some(next) = next
        && program(t, next)
    {
        return true;
    }
    fallback(t, 3);
    true
}
pub(super) fn stop() {
    let _local = Local::acquire();
    let t = local();
    if matches!(t.mode.load(Ordering::Acquire), REPEATING | SINGLE | PAUSED) {
        mask();
        let _ = wr(t, 0);
        t.next.store(0, Ordering::Release);
        t.mode.store(COUNT, Ordering::Release);
    }
}
pub(super) fn active(cpu: u32) -> bool {
    let Some(t) = TIMERS.get(cpu as usize) else {
        return false;
    };
    matches!(t.mode.load(Ordering::Acquire), REPEATING | SINGLE)
        && t.epoch.load(Ordering::Acquire) == cpu_topology::epoch(cpu)
        && clocksource::deadline_clock(cpu)
            .is_some_and(|(_, g)| g == t.generation.load(Ordering::Acquire))
}
pub(super) fn idle_budget_us() -> Option<u64> {
    let t = local();
    if !current(t) || !active(percpu::current_index()) {
        return None;
    }
    let next = t.next.load(Ordering::Acquire);
    let now = super::read_tsc();
    let hz = t.frequency.load(Ordering::Acquire);
    if next <= now || hz == 0 {
        return None;
    }
    u64::try_from(u128::from(next - now) * 1_000_000 / u128::from(hz)).ok()
}
pub(crate) fn log(cpu: u32) {
    use core::fmt::Write;
    let Some(t) = TIMERS.get(cpu as usize) else {
        return;
    };
    let mut line = crate::boot::log::Line::<256>::new();
    write!(&mut line,
        "[TSCDEADLINE] cpu={} active={} epoch={} generation={} arms={} irqs={} stale={} fallbacks={} reason={}\r\n",
        cpu, u8::from(active(cpu)), t.epoch.load(Ordering::Acquire),
        t.generation.load(Ordering::Acquire), t.arms.load(Ordering::Relaxed),
        t.irqs.load(Ordering::Relaxed), t.stale.load(Ordering::Relaxed),
        t.fallbacks.load(Ordering::Relaxed), t.reason.load(Ordering::Acquire)
    ).expect("Bounded CPU timer diagnostic");
    crate::boot::log::text(line.as_bytes());
    crate::boot::log_route::serial_write_raw(line.as_bytes());
}
