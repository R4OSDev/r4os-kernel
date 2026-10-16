//! Original early/runtime USB deadlines. Local IF is sampled only briefly;
//! no runtime/no-sleep owner spans a wait, device callback or scheduler park.
use super::{
    timing,
    wait_types::{Deadline, Metrics, Wait, WaitResult},
};
use crate::{
    arch::{hpet, runtime},
    memory::owner::{Lock, Token},
    platform::{cpu, monotonic, timer},
    sched::{scheduler, sync_types::Text},
};
use core::cell::UnsafeCell;
const IF: u64 = 1 << 9;
const RUNTIME_POLL_GRACE_NS: u64 = 1_000_000;
const RUNTIME_POLL_FALLBACK_ITERATIONS: u64 = 100_000;
const NO_CLOCK_SPINS_PER_MS: u64 = 100_000;
#[expect(
    clippy::manual_div_ceil,
    reason = "Retain the original checked rounding expression during migration."
)]
fn cycles(frequency: u64, milliseconds: u32, maximum: u64) -> u64 {
    if frequency == 0 || milliseconds == 0 {
        return 0;
    }
    let value = (u128::from(frequency) * u128::from(milliseconds) + 999) / 1000;
    if value == 0 || value > u128::from(maximum) {
        0
    } else {
        value as u64
    }
}
fn exact_tsc(info: cpu::Status) -> u64 {
    if info.tsc_denominator != 0 && info.tsc_numerator != 0 && info.crystal_hz != 0 {
        let value = u128::from(info.crystal_hz) * u128::from(info.tsc_numerator)
            / u128::from(info.tsc_denominator);
        if (1_000_000..=10_000_000_000).contains(&value) {
            return value as u64;
        }
    }
    0
}
fn read_tsc() -> u64 {
    let (low, high): (u32, u32);
    unsafe {
        core::arch::asm!("lfence","rdtsc",out("eax")low,out("edx")high,options(nostack,preserves_flags))
    };
    (u64::from(high) << 32) | u64::from(low)
}
impl Deadline {
    pub fn begin(milliseconds: u32) -> Self {
        let flags = runtime::save_local();
        let may_enable = flags & IF != 0 || scheduler::current().is_null();
        let start_monotonic = monotonic::capture();
        let mono = monotonic::snapshot();
        let monotonic_valid = monotonic::resolve(start_monotonic).is_some()
            && (may_enable || mono.flags & (1 << 3) != 0);
        let hp = hpet::snapshot();
        let hpet_mask = if hp.flags & 16 != 0 {
            u64::MAX
        } else {
            u64::from(u32::MAX)
        };
        let duration_hpet = if hp.flags & 2 != 0 && hp.flags & 4 != 0 && hp.frequency_hz != 0 {
            cycles(hp.frequency_hz, milliseconds, hpet_mask)
        } else {
            0
        };
        let duration_ticks = if may_enable {
            timing::ticks_for_milliseconds(milliseconds, timer::frequency())
        } else {
            0
        };
        let info = cpu::snapshot();
        let trusted = info.features.tsc && info.features.invariant_tsc;
        let exact = if trusted { exact_tsc(info) } else { 0 };
        let wall_clock = duration_ticks != 0 || duration_hpet != 0;
        let use_tsc = info.features.tsc && (trusted || !wall_clock);
        let fallback_tsc = use_tsc && exact == 0;
        let duration_tsc = if use_tsc {
            cycles(
                if exact != 0 { exact } else { 10_000_000_000 },
                milliseconds,
                u64::MAX,
            )
        } else {
            0
        };
        let out = Self {
            flags,
            start_monotonic,
            duration_ns: u64::from(milliseconds) * 1_000_000,
            monotonic_valid,
            start_tick: timer::tick_count(),
            duration_ticks,
            start_hpet: if duration_hpet != 0 {
                hpet::main_counter() & hpet_mask
            } else {
                0
            },
            duration_hpet,
            hpet_mask,
            start_tsc: if duration_tsc != 0 { read_tsc() } else { 0 },
            duration_tsc,
            fallback_tsc,
        };
        runtime::restore_local(flags);
        if may_enable {
            runtime::sti();
        }
        out
    }
    pub fn expired(&self) -> bool {
        self.duration_ticks != 0
            && timer::tick_count().wrapping_sub(self.start_tick) >= self.duration_ticks
    }
    pub fn monotonic_expired(&self) -> bool {
        self.monotonic_valid
            && self.duration_ns != 0
            && monotonic::elapsed(self.start_monotonic, monotonic::capture())
                .is_some_and(|elapsed| elapsed >= self.duration_ns)
    }
    pub fn uses_monotonic_deadline(&self) -> bool {
        self.monotonic_valid && self.duration_ns != 0
    }
    pub fn uses_tick_deadline(&self) -> bool {
        self.duration_ticks != 0
    }
    pub fn hpet_expired(&self) -> bool {
        self.duration_hpet != 0 && self.elapsed_hpet() >= self.duration_hpet
    }
    pub fn uses_hpet_deadline(&self) -> bool {
        self.duration_hpet != 0
    }
    pub fn tsc_expired(&self) -> bool {
        self.duration_tsc != 0 && self.elapsed_tsc() >= self.duration_tsc
    }
    pub fn uses_tsc_deadline(&self) -> bool {
        self.duration_tsc != 0
    }
    pub fn uses_fallback_tsc(&self) -> bool {
        self.fallback_tsc
    }
    pub fn has_clock(&self) -> bool {
        self.monotonic_valid
            || self.duration_ticks != 0
            || self.duration_hpet != 0
            || self.duration_tsc != 0
    }
    pub fn expired_any(&self) -> bool {
        self.monotonic_expired() || self.expired() || self.hpet_expired() || self.tsc_expired()
    }
    pub fn elapsed_nanoseconds(&self) -> u64 {
        if self.monotonic_valid
            && let Some(value) = monotonic::elapsed(self.start_monotonic, monotonic::capture())
        {
            return value;
        }
        if self.duration_ticks != 0 && timer::frequency() != 0 {
            return (u128::from(self.elapsed_ticks()) * 1_000_000_000
                / u128::from(timer::frequency()))
            .min(u128::from(u64::MAX)) as u64;
        }
        0
    }
    pub fn can_block(&self) -> bool {
        !scheduler::current().is_null() && self.flags & IF != 0
    }
    pub fn wait_step(&self, reason: Text) -> bool {
        if self.can_block() {
            unsafe { scheduler::sleep(1, reason.pointer, reason.len) };
            return true;
        }
        core::hint::spin_loop();
        false
    }
    pub fn elapsed_ticks(&self) -> u64 {
        timer::tick_count().wrapping_sub(self.start_tick)
    }
    pub fn elapsed_hpet(&self) -> u64 {
        if self.duration_hpet == 0 {
            0
        } else {
            (hpet::main_counter() & self.hpet_mask).wrapping_sub(self.start_hpet) & self.hpet_mask
        }
    }
    pub fn elapsed_tsc(&self) -> u64 {
        if self.duration_tsc == 0 {
            0
        } else {
            read_tsc().wrapping_sub(self.start_tsc)
        }
    }
    pub fn finish(&self) {
        if self.flags & IF != 0 {
            runtime::sti();
        } else {
            runtime::cli();
        }
    }
}
struct Storage(UnsafeCell<Metrics>);
// SAFETY: METRICS_OWNER covers only RAM copy/update, never clocks or waits.
unsafe impl Sync for Storage {}
static METRICS: Storage = Storage(UnsafeCell::new(Metrics::EMPTY));
static METRICS_OWNER: Lock = Lock::new(3, 2);
struct MetricsGuard(Token);
impl MetricsGuard {
    fn acquire() -> Self {
        Self(METRICS_OWNER.acquire())
    }
}
impl Drop for MetricsGuard {
    fn drop(&mut self) {
        METRICS_OWNER.release(self.0);
    }
}
pub fn reset_metrics() {
    let _guard = MetricsGuard::acquire();
    unsafe { METRICS.0.get().write(Metrics::EMPTY) };
}
pub fn metrics() -> Metrics {
    let _guard = MetricsGuard::acquire();
    unsafe { METRICS.0.get().read() }
}
fn record(result: WaitResult) {
    let _guard = MetricsGuard::acquire();
    let m = unsafe { &mut *METRICS.0.get() };
    m.calls = m.calls.wrapping_add(1);
    if result.success {
        m.successes = m.successes.wrapping_add(1);
    } else {
        m.timeouts = m.timeouts.wrapping_add(1);
    }
    match result.phase {
        0 => m.early_calls = m.early_calls.wrapping_add(1),
        1 => m.runtime_calls = m.runtime_calls.wrapping_add(1),
        _ => panic!("USB wait phase"),
    }
    m.requested_ms = m.requested_ms.wrapping_add(u64::from(result.requested_ms));
    m.elapsed_ns = m.elapsed_ns.wrapping_add(result.elapsed_ns);
    m.max_elapsed_ns = m.max_elapsed_ns.max(result.elapsed_ns);
    m.iterations = m.iterations.wrapping_add(result.iterations);
    m.blocked_ticks = m.blocked_ticks.wrapping_add(result.blocked_ticks);
    m.last_reason = result.reason;
    m.last_phase = result.phase;
    m.last_requested_ms = result.requested_ms;
    m.last_elapsed_ns = result.elapsed_ns;
    m.last_iterations = result.iterations;
    m.last_blocked_ticks = result.blocked_ticks;
    m.last_retry = result.retry;
    m.last_success = result.success;
}
impl Wait {
    pub fn begin(milliseconds: u32, reason: Text, retry: u8) -> Self {
        let phase = u8::from(!scheduler::current().is_null());
        Self {
            deadline: Deadline::begin(milliseconds),
            reason,
            phase,
            requested_ms: milliseconds,
            iterations: 0,
            blocked_ticks: 0,
            retry,
            runtime_poll_grace: true,
        }
    }
    pub fn expired(&self) -> bool {
        if self.deadline.has_clock() {
            self.deadline.expired_any()
        } else {
            self.iterations >= u64::from(self.requested_ms) * NO_CLOCK_SPINS_PER_MS
        }
    }
    pub fn idle(&mut self) {
        self.iterations = self.iterations.wrapping_add(1);
        if self.phase == 1
            && self.runtime_poll_grace
            && self.deadline.can_block()
            && !self.runtime_poll_grace_expired()
        {
            core::hint::spin_loop();
            return;
        }
        if self.deadline.wait_step(self.reason) {
            self.blocked_ticks = self.blocked_ticks.wrapping_add(1);
        }
    }
    pub fn may_block_on_idle(&self) -> bool {
        self.deadline.can_block()
            && (self.phase != 1
                || !self.runtime_poll_grace
                || self.runtime_poll_grace_expired()
                || self.iterations.saturating_add(1) >= RUNTIME_POLL_FALLBACK_ITERATIONS)
    }
    pub fn poll_once(&mut self) {
        self.iterations = self.iterations.wrapping_add(1);
        core::hint::spin_loop();
    }
    fn runtime_poll_grace_expired(&self) -> bool {
        let elapsed = self.deadline.elapsed_nanoseconds();
        if elapsed != 0 {
            elapsed >= RUNTIME_POLL_GRACE_NS
        } else {
            self.iterations >= RUNTIME_POLL_FALLBACK_ITERATIONS
        }
    }
    pub fn finish(&mut self, success: bool) -> WaitResult {
        let result = WaitResult {
            reason: self.reason,
            phase: self.phase,
            requested_ms: self.requested_ms,
            elapsed_ns: self.deadline.elapsed_nanoseconds(),
            iterations: self.iterations,
            blocked_ticks: self.blocked_ticks,
            retry: self.retry,
            success,
        };
        self.deadline.finish();
        record(result);
        result
    }
}
pub fn milliseconds(value: u32, reason: Text, retry: u8) -> WaitResult {
    if value == 0 {
        let result = WaitResult {
            reason,
            phase: u8::from(!scheduler::current().is_null()),
            requested_ms: 0,
            elapsed_ns: 0,
            iterations: 0,
            blocked_ticks: 0,
            retry,
            success: true,
        };
        record(result);
        return result;
    }
    let mut wait = Wait::begin(value, reason, retry);
    wait.runtime_poll_grace = false;
    while !wait.expired() {
        wait.idle();
    }
    wait.finish(true)
}

/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_begin(ms: u32, out: *mut Deadline) {
    unsafe { out.write(Deadline::begin(ms)) }
}
// Each private entry below names its typed operation; no numeric dispatch.
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_expired(d: *const Deadline) -> bool {
    unsafe { (*d).expired() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_monotonic_expired(d: *const Deadline) -> bool {
    unsafe { (*d).monotonic_expired() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_uses_monotonic_deadline(
    d: *const Deadline,
) -> bool {
    unsafe { (*d).uses_monotonic_deadline() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_uses_tick_deadline(d: *const Deadline) -> bool {
    unsafe { (*d).uses_tick_deadline() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_hpet_expired(d: *const Deadline) -> bool {
    unsafe { (*d).hpet_expired() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_uses_hpet_deadline(d: *const Deadline) -> bool {
    unsafe { (*d).uses_hpet_deadline() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_tsc_expired(d: *const Deadline) -> bool {
    unsafe { (*d).tsc_expired() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_uses_tsc_deadline(d: *const Deadline) -> bool {
    unsafe { (*d).uses_tsc_deadline() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_uses_fallback_tsc(d: *const Deadline) -> bool {
    unsafe { (*d).uses_fallback_tsc() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_has_clock(d: *const Deadline) -> bool {
    unsafe { (*d).has_clock() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_expired_any(d: *const Deadline) -> bool {
    unsafe { (*d).expired_any() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_elapsed_nanoseconds(d: *const Deadline) -> u64 {
    unsafe { (*d).elapsed_nanoseconds() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_can_block(d: *const Deadline) -> bool {
    unsafe { (*d).can_block() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_elapsed_ticks(d: *const Deadline) -> u64 {
    unsafe { (*d).elapsed_ticks() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_elapsed_hpet(d: *const Deadline) -> u64 {
    unsafe { (*d).elapsed_hpet() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_elapsed_tsc(d: *const Deadline) -> u64 {
    unsafe { (*d).elapsed_tsc() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_step(
    d: *const Deadline,
    text: *const u8,
    len: usize,
) -> bool {
    unsafe { (*d).wait_step(Text { pointer: text, len }) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_deadline_finish(d: *const Deadline) {
    unsafe { (*d).finish() }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_wait_reset_metrics() {
    reset_metrics()
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_wait_metrics(out: *mut Metrics) {
    unsafe { out.write(metrics()) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_wait_begin(
    ms: u32,
    text: *const u8,
    len: usize,
    retry: u8,
    out: *mut Wait,
) {
    unsafe { out.write(Wait::begin(ms, Text { pointer: text, len }, retry)) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_wait_expired(w: *const Wait) -> bool {
    unsafe { (*w).expired() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_wait_idle(w: *mut Wait) {
    unsafe { (&mut *w).idle() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_wait_may_block(w: *const Wait) -> bool {
    unsafe { (*w).may_block_on_idle() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_wait_poll_once(w: *mut Wait) {
    unsafe { (&mut *w).poll_once() }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_wait_finish(
    w: *mut Wait,
    success: bool,
    out: *mut WaitResult,
) {
    unsafe { out.write((&mut *w).finish(success)) }
}
/// # Safety
/// Exact live private C records and resident labels/buffers; outputs are
/// exclusive. The caller retains the owner/context documented by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_wait_milliseconds(
    ms: u32,
    text: *const u8,
    len: usize,
    retry: u8,
    out: *mut WaitResult,
) {
    unsafe { out.write(milliseconds(ms, Text { pointer: text, len }, retry)) }
}
