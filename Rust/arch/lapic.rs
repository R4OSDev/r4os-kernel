//! CPU-local APIC registers/IPIs, CPU-local deadline timers and the retained
//! BSP count-timer snapshot. EOI/TLB IPIs never acquire runtime.
mod deadline;
mod deadline_math;
use super::{apic_target, hpet, msr, percpu, runtime::RuntimeGuard};
use crate::{
    build_config::config,
    memory::{paging, phys},
};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
const AVAILABLE: u32 = 1;
const ENABLED: u32 = 2;
const SOFTWARE: u32 = 4;
const TIMER_ENABLED: u32 = 8;
const CALIBRATED: u32 = 16;
const ONE_SHOT: u32 = 32;
const BSP: u32 = 64;
const X2APIC: u32 = 128;
const TSC_DEADLINE: u32 = 256;
const APIC_BASE: u32 = 0x1b;
const BASE_ENABLE: u64 = 1 << 11;
const BASE_X2: u64 = 1 << 10;
const ID: u32 = 0x20;
const VERSION: u32 = 0x30;
const EOI: u32 = 0xb0;
const SVR: u32 = 0xf0;
const ICR: u32 = 0x300;
const LVT: u32 = 0x320;
const INITIAL: u32 = 0x380;
const CURRENT: u32 = 0x390;
const DIVIDE: u32 = 0x3e0;
const VECTOR: u32 = 0x20;
const MASKED: u32 = 1 << 16;
const PERIODIC: u32 = 1 << 17;
const MIN_COUNT: u32 = 16;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub phys_base: u64,
    pub msr_base: u64,
    pub virt_base: u64,
    pub timer_ticks: u64,
    pub calibration_hpet_ticks: u64,
    pub flags: u32,
    pub svr: u32,
    pub timer_frequency_hz: u32,
    pub timer_initial_count: u32,
    pub timer_current_count: u32,
    pub timer_divide: u32,
    pub timer_lvt: u32,
    pub calibration_lapic_ticks: u32,
    pub id: u32,
    pub version: u8,
    pub max_lvt: u8,
    pub timer_vector: u8,
    pub reason: u8,
    pub timer_reason: u8,
    pub reserved: [u8; 2],
}
impl Status {
    const EMPTY: Self = Self {
        phys_base: 0,
        msr_base: 0,
        virt_base: 0,
        timer_ticks: 0,
        calibration_hpet_ticks: 0,
        flags: 0,
        svr: 0,
        timer_frequency_hz: 0,
        timer_initial_count: 0,
        timer_current_count: 0,
        timer_divide: 3,
        timer_lvt: 0,
        calibration_lapic_ticks: 0,
        id: 0,
        version: 0,
        max_lvt: 0,
        timer_vector: 0x20,
        reason: 0,
        timer_reason: 0,
        reserved: [0; 2],
    };
    fn has(self, flags: u32) -> bool {
        self.flags & flags != 0
    }
    fn set(&mut self, flag: u32, on: bool) {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }
}
struct Storage(UnsafeCell<Status>);
// SAFETY: BSP boot or runtime admission owns STATE. CPU-local register work
// only consumes atomic immutable base/mode; APs never write this BSP catalog.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(Status::EMPTY));
static BASE: AtomicU64 = AtomicU64::new(0);
static MODE_X2: AtomicBool = AtomicBool::new(false);
static FLAGS: AtomicU32 = AtomicU32::new(0);
static TICKS: AtomicU64 = AtomicU64::new(0);
static CURRENT_COUNT: AtomicU32 = AtomicU32::new(0);
static HZ: AtomicU32 = AtomicU32::new(0);
static IDLE_COUNT_RATE: AtomicU64 = AtomicU64::new(0);
static BSP_ID: AtomicU64 = AtomicU64::new(u64::MAX);

fn state() -> Status {
    let mut value = unsafe { *STATE.0.get() };
    value.timer_ticks = TICKS.load(Ordering::Relaxed);
    value.timer_current_count = CURRENT_COUNT.load(Ordering::Relaxed);
    value.set(TSC_DEADLINE, deadline::active(0));
    if !value.has(TSC_DEADLINE) && value.timer_reason == 14 {
        value.timer_reason = 15;
    }
    value
}
fn publish(value: Status) {
    unsafe { STATE.0.get().write(value) };
    CURRENT_COUNT.store(value.timer_current_count, Ordering::Relaxed);
    HZ.store(value.timer_frequency_hz, Ordering::Release);
    // Rounded periodic calibration plus one tick's rounding margin yields a
    // conservative upper rate, hence a lower remaining-time budget.
    IDLE_COUNT_RATE.store(
        u64::from(value.timer_initial_count)
            .saturating_add(1)
            .saturating_mul(u64::from(value.timer_frequency_hz)),
        Ordering::Release,
    );
    FLAGS.store(value.flags, Ordering::Release);
}
fn log(event: u8, value: Status) {
    unsafe { lapic_log(event, &value) };
}
fn read(register: u32) -> u32 {
    if MODE_X2.load(Ordering::Acquire) {
        return unsafe { msr::read(0x800 + (register >> 4)) } as u32;
    }
    unsafe {
        core::ptr::with_exposed_provenance::<u32>(
            (BASE.load(Ordering::Acquire) + u64::from(register)) as usize,
        )
        .read_volatile()
    }
}
fn write(register: u32, value: u32) {
    if MODE_X2.load(Ordering::Acquire) {
        unsafe { msr::write(0x800 + (register >> 4), u64::from(value)) };
        return;
    }
    unsafe {
        core::ptr::with_exposed_provenance_mut::<u32>(
            (BASE.load(Ordering::Acquire) + u64::from(register)) as usize,
        )
        .write_volatile(value)
    };
}
pub fn is_enabled() -> bool {
    FLAGS.load(Ordering::Acquire) & (ENABLED | SOFTWARE) == (ENABLED | SOFTWARE)
        && (BASE.load(Ordering::Acquire) != 0 || MODE_X2.load(Ordering::Acquire))
}
/// IF0 local observation only. A masked/disarmed/replaced timer never proves
/// an idle wake deadline. ARAT is separately mandatory at CPU entry admission.
pub(crate) fn idle_budget_us() -> Option<u64> {
    if !is_enabled() {
        return None;
    }
    let lvt = read(LVT);
    if lvt & MASKED != 0 || lvt & 255 != VECTOR {
        return None;
    }
    if lvt & (1 << 18) != 0 {
        return deadline::idle_budget_us();
    }
    let rate = IDLE_COUNT_RATE.load(Ordering::Acquire);
    let count = read(CURRENT);
    if count == 0 || rate == 0 {
        return None;
    }
    Some(u64::from(count).saturating_mul(1_000_000) / rate)
}
pub(crate) fn bsp_apic_id() -> Option<u32> {
    if !is_enabled() {
        return None;
    }
    u32::try_from(BSP_ID.load(Ordering::Acquire)).ok()
}
fn local_id() -> u32 {
    if MODE_X2.load(Ordering::Acquire) {
        read(ID)
    } else {
        read(ID) >> 24
    }
}
pub(crate) fn can_target(id: u32) -> bool {
    is_enabled() && apic_target::icr(u64::from(id), 0xf0, MODE_X2.load(Ordering::Acquire)).is_some()
}
pub(crate) fn local_capabilities_valid() -> bool {
    let features = core::arch::x86_64::__cpuid_count(1, 0);
    features.edx & ((1 << 5) | (1 << 9)) == (1 << 5) | (1 << 9)
        && (!MODE_X2.load(Ordering::Acquire) || features.ecx & (1 << 21) != 0)
}
fn local_deadline_capable() -> bool {
    let zero = core::arch::x86_64::__cpuid_count(0, 0);
    if zero.eax < 1 {
        return false;
    }
    let one = core::arch::x86_64::__cpuid_count(1, 0);
    let intel = zero.ebx == u32::from_le_bytes(*b"Genu")
        && zero.edx == u32::from_le_bytes(*b"ineI")
        && zero.ecx == u32::from_le_bytes(*b"ntel");
    let maximum = core::arch::x86_64::__cpuid_count(0x8000_0000, 0).eax;
    let invariant = (0x8000_0007..=0x8fff_ffff).contains(&maximum)
        && core::arch::x86_64::__cpuid_count(0x8000_0007, 0).edx & (1 << 8) != 0;
    deadline_math::hardware_allowed(one.ecx, one.edx, one.eax, intel, invariant)
}
fn read_tsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("lfence", "rdtsc", out("eax") low, out("edx") high, options(nostack, preserves_flags));
    }
    (u64::from(high) << 32) | u64::from(low)
}
fn deadline_fence() {
    unsafe {
        core::arch::asm!("mfence", options(nostack, preserves_flags));
    }
}
/// Pre-topology timer handoff only selects the already validated count path.
/// Actual deadline arm still waits for the later nonzero CPU epoch.
pub(crate) fn deadline_candidate() -> bool {
    percpu::current_index() == 0
        && is_enabled()
        && local_deadline_capable()
        && crate::platform::clocksource::deadline_clock(0).is_some()
}
/// The BSP starts with the validated count timer. Select deadline mode only
/// after topology has supplied a nonzero exact CPU epoch.
pub(crate) fn qualify_bsp_timer() {
    let _owner = RuntimeGuard::acquire();
    if percpu::current_index() != 0 {
        return;
    }
    if crate::platform::timer::active_backend() != 2 {
        deadline::external_backend();
        return;
    }
    let mut value = state();
    if value.has(CALIBRATED)
        && value.has(TIMER_ENABLED)
        && !value.has(ONE_SHOT)
        && deadline::start(value.timer_initial_count, value.timer_frequency_hz)
    {
        value.flags |= TSC_DEADLINE;
        value.timer_lvt = read(LVT);
        value.timer_reason = 14;
        publish(value);
    }
}
pub(crate) fn on_timer_irq() -> bool {
    deadline::on_irq()
}
pub(crate) fn revalidate_current_timer() {
    deadline::revalidate();
}
pub(crate) fn log_timer(cpu: u32) {
    deadline::log(cpu);
}
pub(crate) fn deadline_boot_check() -> bool {
    deadline::boot_check()
}
fn init_current() -> bool {
    if BASE.load(Ordering::Acquire) == 0 && !MODE_X2.load(Ordering::Acquire) {
        return false;
    }
    let mut base = unsafe { msr::read(APIC_BASE) } | BASE_ENABLE;
    if MODE_X2.load(Ordering::Acquire) {
        base |= BASE_X2;
    }
    unsafe { msr::write(APIC_BASE, base) };
    write(SVR, (read(SVR) & !0xff) | 0x100 | 0xff);
    read(SVR) & 0x100 != 0
        && local_id() == crate::platform::cpu::local_apic_id()
        && u64::from(local_id())
            == percpu::r4os_rust_percpu_control(8, percpu::current_index(), 0, 0)
}
fn map(base: u64) -> bool {
    let page = base & !4095;
    let virtual_page = phys::r4os_rust_phys_to_virt(page);
    if !paging::ensure_mmio(virtual_page, page) {
        return false;
    }
    BASE.store(phys::r4os_rust_phys_to_virt(base), Ordering::Release);
    true
}
pub fn init(
    address: u64,
    enabled_count: u32,
    supports_x2: bool,
    topology_needs_x2: bool,
) -> Status {
    let _owner = RuntimeGuard::acquire();
    let mut value = Status {
        phys_base: address,
        reason: 1,
        ..Status::EMPTY
    };
    BASE.store(0, Ordering::Release);
    MODE_X2.store(false, Ordering::Release);
    BSP_ID.store(u64::MAX, Ordering::Release);
    TICKS.store(0, Ordering::Relaxed);
    if address == 0 || enabled_count == 0 {
        publish(value);
        log(0, value);
        return value;
    }
    value.flags |= AVAILABLE;
    let apic_base = unsafe { msr::read(APIC_BASE) };
    value.msr_base = apic_base & 0x000f_ffff_ffff_f000;
    value.set(BSP, apic_base & (1 << 8) != 0);
    value.set(X2APIC, apic_base & BASE_X2 != 0);
    value.set(ENABLED, apic_base & BASE_ENABLE != 0);
    if value.phys_base == 0 {
        value.phys_base = value.msr_base;
    }
    let expected = crate::platform::cpu::local_apic_id();
    let needs_x2 = apic_target::legacy_id(u64::from(expected)).is_none();
    if (config::FORCE_X2APIC || needs_x2 || topology_needs_x2) && !value.has(X2APIC) && supports_x2
    {
        unsafe { msr::write(APIC_BASE, apic_base | BASE_ENABLE | BASE_X2) };
        let forced = unsafe { msr::read(APIC_BASE) };
        value.set(X2APIC, forced & BASE_X2 != 0);
        value.set(ENABLED, forced & BASE_ENABLE != 0);
    }
    if expected == u32::MAX
        || (needs_x2 && !value.has(X2APIC))
        || (value.has(X2APIC) && !supports_x2)
    {
        crate::boot::fatal::kernel(11, b"bsp-apic-transport");
    }
    if value.has(X2APIC) {
        MODE_X2.store(true, Ordering::Release);
        if !value.has(ENABLED) {
            unsafe { msr::write(APIC_BASE, msr::read(APIC_BASE) | BASE_ENABLE | BASE_X2) };
            value.flags |= ENABLED;
        }
        value.id = read(ID);
        value.reason = 2;
    } else {
        if value.phys_base == 0 {
            value.reason = 3;
            publish(value);
            log(0, value);
            return value;
        }
        if !map(value.phys_base) {
            value.reason = 4;
            publish(value);
            log(0, value);
            return value;
        }
        value.virt_base = BASE.load(Ordering::Acquire);
        if !value.has(ENABLED) {
            unsafe { msr::write(APIC_BASE, apic_base | BASE_ENABLE) };
            value.flags |= ENABLED;
        }
        value.id = read(ID) >> 24;
        value.reason = 5;
    }
    if value.id != expected || u64::from(expected) != percpu::r4os_rust_percpu_control(8, 0, 0, 0) {
        crate::boot::fatal::kernel(11, b"bsp-apic-identity");
    }
    BSP_ID.store(u64::from(value.id), Ordering::Release);
    let version = read(VERSION);
    value.version = version as u8;
    value.max_lvt = (version >> 16) as u8;
    write(SVR, (read(SVR) & !0xff) | 0x100 | 0xff);
    value.svr = read(SVR);
    value.set(SOFTWARE, value.svr & 0x100 != 0);
    publish(value);
    log(0, value);
    value
}
fn hpet_reference() -> Option<u64> {
    let reference = hpet::snapshot();
    (reference.flags & 4 != 0 && reference.frequency_hz != 0).then_some(reference.frequency_hz)
}
fn calibration_sample(delta: u64) -> (u64, u32) {
    write(DIVIDE, 3);
    write(LVT, VECTOR | MASKED);
    write(INITIAL, u32::MAX);
    let start = hpet::main_counter();
    while hpet::elapsed(start, hpet::main_counter()) < delta {
        core::hint::spin_loop();
    }
    (
        hpet::elapsed(start, hpet::main_counter()),
        u32::MAX.wrapping_sub(read(CURRENT)),
    )
}
fn calibrated_count(hpet: u64, lapic: u32, frequency: u64, hz: u32) -> u128 {
    (u128::from(lapic) * u128::from(frequency)) / (u128::from(hpet) * u128::from(hz))
}
fn start_secondary(requested_hz: u32) -> bool {
    if !init_current() {
        return false;
    }
    let Some(frequency) = hpet_reference() else {
        return false;
    };
    let hz = if requested_hz == 0 { 100 } else { requested_hz };
    let delta = frequency / 100;
    if delta == 0 {
        return false;
    }
    write(INITIAL, 0);
    let (elapsed_hpet, elapsed_lapic) = calibration_sample(delta);
    write(INITIAL, 0);
    if elapsed_hpet == 0 || elapsed_lapic < MIN_COUNT {
        return false;
    }
    let initial = calibrated_count(elapsed_hpet, elapsed_lapic, frequency, hz);
    if initial < u128::from(MIN_COUNT) || initial > u128::from(0xffff_fffeu32) {
        return false;
    }
    write(DIVIDE, 3);
    write(LVT, VECTOR | PERIODIC);
    write(INITIAL, initial as u32);
    let _ = deadline::start(initial as u32, hz);
    true
}
pub(crate) fn secondary_timer_sample(cpu: u32) -> Option<(u64, u64, u64, u32, u32)> {
    deadline::timer_sample(cpu)
}
pub(crate) struct PausedTimer {
    cpu: u32,
    divider: u32,
    lvt: u32,
    initial: u32,
    epoch: u64,
    deadline: Option<deadline::Paused>,
}
impl PausedTimer {
    pub(crate) fn clock_current(&self) -> bool {
        self.deadline
            .as_ref()
            .is_none_or(deadline::paused_clock_current)
    }
    pub(crate) fn matches_current(&self) -> bool {
        self.cpu == percpu::current_index()
            && self.epoch != 0
            && self.epoch == crate::platform::cpu_topology::epoch(self.cpu)
    }
}

/// Scheduler idle loan, local IF=0. None means no required local event; a
/// finite retry is a one-shot, never a new fixed periodic idle tick.
pub(crate) fn arm_secondary_idle(token: &PausedTimer, ticks: Option<u64>) -> bool {
    if !token.matches_current() {
        return false;
    }
    if let Some(deadline) = token.deadline.as_ref() {
        // A failed deadline arm must consume/restore this exact loan first.
        // Its hardware INITIAL register can be zero; count fallback uses the
        // calibrated owner's state, never that deadline-mode register value.
        return deadline::arm_idle(deadline, ticks);
    }
    write(DIVIDE, token.divider);
    let lvt = token.lvt & !(PERIODIC | (1 << 18) | MASKED);
    write(LVT, lvt | MASKED);
    write(INITIAL, 0);
    if let Some(ticks) = ticks {
        let count = u64::from(token.initial)
            .saturating_mul(ticks.max(1))
            .min(u64::from(u32::MAX - 1)) as u32;
        if count == 0 {
            return false;
        }
        write(INITIAL, count);
        write(LVT, lvt);
        read(LVT) == lvt && read(INITIAL) == count
    } else {
        read(LVT) & MASKED != 0 && read(INITIAL) == 0
    }
}

/// Restore the exact AP's active periodic quantum timer before any productive
/// stack can run. This consumes the scheduler loan before lifecycle can pause.
pub(crate) fn finish_secondary_idle(token: PausedTimer) -> bool {
    if !token.matches_current() {
        return false;
    }
    if let Some(deadline) = token.deadline {
        deadline::finish_idle(deadline)
    } else {
        resume_secondary(token)
    }
}
/// Local IF=0 quiescent AP only. Do not mutate the BSP timer snapshot.
pub(crate) fn pause_secondary() -> Option<PausedTimer> {
    let cpu = percpu::current_index();
    if cpu == 0 || !is_enabled() {
        return None;
    }
    let paused = deadline::pause().ok()?;
    let token = PausedTimer {
        cpu,
        divider: read(DIVIDE),
        lvt: read(LVT),
        initial: read(INITIAL),
        epoch: crate::platform::cpu_topology::epoch(cpu),
        deadline: paused,
    };
    if token.deadline.is_some() {
        return Some(token);
    }
    if token.lvt == u32::MAX {
        return None;
    }
    write(LVT, token.lvt | MASKED);
    write(INITIAL, 0);
    (read(LVT) & MASKED != 0 && read(INITIAL) == 0).then_some(token)
}
/// A pre-withdrawal cancellation restores only this AP's retained timer.
pub(crate) fn resume_secondary(token: PausedTimer) -> bool {
    if token.cpu != percpu::current_index()
        || token.epoch == 0
        || token.epoch != crate::platform::cpu_topology::epoch(token.cpu)
    {
        return false;
    }
    if let Some(deadline) = token.deadline {
        return deadline::resume(deadline);
    }
    write(DIVIDE, token.divider);
    write(LVT, token.lvt | MASKED);
    write(INITIAL, token.initial);
    write(LVT, token.lvt);
    read(DIVIDE) == token.divider && read(LVT) == token.lvt && read(INITIAL) == token.initial
}
fn write_icr(apic_id: u64, low: u32) -> bool {
    if !is_enabled() {
        return false;
    }
    let x2 = MODE_X2.load(Ordering::Acquire);
    let Some(message) = apic_target::icr(apic_id, low, x2) else {
        return false;
    };
    if x2 {
        unsafe { msr::write(0x830, message.msr()) };
        return true;
    }
    if BASE.load(Ordering::Acquire) == 0 {
        return false;
    }
    write(ICR + 0x10, message.high);
    write(ICR, message.low);
    true
}
fn wait_icr() -> bool {
    let mut spins = 0;
    while read(ICR) & (1 << 12) != 0 {
        if spins >= 1_000_000 {
            return false;
        }
        core::hint::spin_loop();
        spins += 1;
    }
    true
}
fn delay_microseconds(microseconds: u64) {
    if let Some(frequency) = hpet_reference() {
        let requested = (u128::from(frequency) * u128::from(microseconds)) / 1_000_000;
        let delta = u64::try_from(requested.max(1)).expect("LAPIC delay exceeds counter width");
        let start = hpet::main_counter();
        while hpet::elapsed(start, hpet::main_counter()) < delta {
            core::hint::spin_loop();
        }
        return;
    }
    let mut spins = microseconds.saturating_mul(256);
    while spins != 0 {
        core::hint::spin_loop();
        spins -= 1;
    }
}
fn init_sipi(apic_id: u64, vector: u8) -> bool {
    if !is_enabled() || vector == 0 {
        return false;
    }
    if !write_icr(apic_id, 0xc500) || !wait_icr() {
        return false;
    }
    delay_microseconds(10_000);
    if !write_icr(apic_id, 0x8500) || !wait_icr() {
        return false;
    }
    delay_microseconds(200);
    if !write_icr(apic_id, 0x600 | u32::from(vector)) || !wait_icr() {
        return false;
    }
    delay_microseconds(200);
    write_icr(apic_id, 0x600 | u32::from(vector)) && wait_icr()
}
fn stop_value(mut value: Status) -> Status {
    deadline::stop();
    if BASE.load(Ordering::Acquire) != 0 || MODE_X2.load(Ordering::Acquire) {
        write(INITIAL, 0);
        write(LVT, VECTOR | MASKED);
        value.timer_lvt = read(LVT);
        value.timer_current_count = read(CURRENT);
    }
    value.flags &= !(TIMER_ENABLED | ONE_SHOT | TSC_DEADLINE);
    value
}
fn timer_failure(mut value: Status, reason: u8) -> bool {
    value.timer_reason = reason;
    publish(value);
    log(1, value);
    false
}
fn init_timer(hz: u32) -> bool {
    let mut value = state();
    if !is_enabled() {
        return timer_failure(value, 1);
    }
    let Some(frequency) = hpet_reference() else {
        return timer_failure(value, 2);
    };
    let hz = if hz == 0 { 100 } else { hz };
    let delta = frequency / 100;
    if delta == 0 {
        return timer_failure(value, 3);
    }
    value = stop_value(value);
    let (elapsed_hpet, elapsed_lapic) = calibration_sample(delta);
    value = stop_value(value);
    if elapsed_hpet == 0 || elapsed_lapic < MIN_COUNT {
        return timer_failure(value, 4);
    }
    let denominator = u128::from(elapsed_hpet) * u128::from(hz);
    if denominator == 0 {
        return timer_failure(value, 5);
    }
    let initial = calibrated_count(elapsed_hpet, elapsed_lapic, frequency, hz);
    if initial < u128::from(MIN_COUNT) || initial > u128::from(0xffff_fffeu32) {
        return timer_failure(value, 6);
    }
    value.timer_frequency_hz = hz;
    value.timer_initial_count = initial as u32;
    value.calibration_hpet_ticks = elapsed_hpet;
    value.calibration_lapic_ticks = elapsed_lapic;
    value.flags |= CALIBRATED;
    write(DIVIDE, 3);
    write(LVT, VECTOR | PERIODIC);
    write(INITIAL, value.timer_initial_count);
    value.timer_lvt = read(LVT);
    value.timer_current_count = read(CURRENT);
    value.flags |= TIMER_ENABLED;
    value.flags &= !ONE_SHOT;
    value.timer_reason = 8;
    publish(value);
    log(2, value);
    true
}
fn one_shot(mut value: Status) -> bool {
    if !is_enabled() || !value.has(CALIBRATED) || value.timer_initial_count < MIN_COUNT {
        value.timer_reason = 9;
        publish(value);
        return false;
    }
    if deadline::one_shot() {
        value.timer_lvt = read(LVT);
        value.timer_current_count = 0;
        value.flags |= TIMER_ENABLED | ONE_SHOT | TSC_DEADLINE;
        value.timer_reason = 10;
        publish(value);
        return true;
    }
    value.flags &= !TSC_DEADLINE;
    write(DIVIDE, 3);
    write(INITIAL, 0);
    write(LVT, VECTOR | MASKED);
    value.timer_lvt = read(LVT);
    value.timer_current_count = read(CURRENT);
    value.flags |= TIMER_ENABLED | ONE_SHOT;
    value.timer_reason = 10;
    publish(value);
    true
}
fn arm(ticks: u64) -> u64 {
    arm_absolute(ticks, None)
}
pub(crate) fn arm_absolute(ticks: u64, absolute_ns: Option<u64>) -> u64 {
    if !state().has(ONE_SHOT) && !one_shot(state()) {
        return 0;
    }
    let mut value = state();
    if let Some(ticks) = deadline::arm_with(ticks, absolute_ns) {
        value.timer_lvt = read(LVT);
        value.timer_current_count = 0;
        value.flags |= TIMER_ENABLED | TSC_DEADLINE;
        value.timer_reason = 11;
        publish(value);
        return ticks;
    }
    value.flags &= !TSC_DEADLINE;
    let count_per_tick = u64::from(value.timer_initial_count);
    if count_per_tick == 0 {
        return 0;
    }
    let maximum_ticks = (u64::from(0xffff_fffeu32) / count_per_tick).max(1);
    let ticks = ticks.max(1).min(maximum_ticks);
    let initial = u32::try_from(ticks * count_per_tick).expect("LAPIC timer count overflow");
    write(DIVIDE, 3);
    write(LVT, VECTOR);
    write(INITIAL, initial);
    value.timer_lvt = read(LVT);
    value.timer_current_count = read(CURRENT);
    value.flags |= TIMER_ENABLED;
    value.timer_reason = 11;
    publish(value);
    ticks
}
fn disarm(mut value: Status) {
    if !value.has(ONE_SHOT) {
        return;
    }
    if deadline::disarm() {
        value.timer_lvt = read(LVT);
        value.timer_current_count = 0;
        value.timer_reason = 12;
        publish(value);
        return;
    }
    write(INITIAL, 0);
    write(LVT, VECTOR | MASKED);
    value.timer_lvt = read(LVT);
    value.timer_current_count = read(CURRENT);
    value.timer_reason = 12;
    publish(value);
}
fn resume(mut value: Status) -> bool {
    if !is_enabled() || !value.has(CALIBRATED) || value.timer_initial_count < MIN_COUNT {
        value.timer_reason = 13;
        publish(value);
        return false;
    }
    if deadline::resume_periodic() {
        value.timer_lvt = read(LVT);
        value.timer_current_count = 0;
        value.flags |= TIMER_ENABLED | TSC_DEADLINE;
        value.flags &= !ONE_SHOT;
        value.timer_reason = 14;
        publish(value);
        return true;
    }
    value.flags &= !TSC_DEADLINE;
    write(DIVIDE, 3);
    write(LVT, VECTOR | PERIODIC);
    write(INITIAL, value.timer_initial_count);
    value.timer_lvt = read(LVT);
    value.timer_current_count = read(CURRENT);
    value.flags |= TIMER_ENABLED;
    value.flags &= !ONE_SHOT;
    value.timer_reason = 8;
    publish(value);
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_lapic_control(operation: u8, a: u64, b: u32) -> u64 {
    // Local register/IPI operations stay independent of runtime admission.
    match operation {
        0 => return u64::from(is_enabled()),
        1 => return u64::from(init_current()),
        2 => {
            return if is_enabled() {
                u64::from(local_id())
            } else {
                u64::MAX
            };
        }
        3 => return u64::from(start_secondary(b)),
        4 => return u64::from(u8::try_from(b).is_ok_and(|vector| init_sipi(a, vector))),
        5 | 6 => return u64::from(write_icr(a, b) && wait_icr()),
        7 => return u64::from(write_icr(a, b) && wait_icr()),
        8 => {
            if is_enabled() {
                write(EOI, 0);
            }
            return 0;
        }
        9 => {
            let ticks = TICKS.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
            CURRENT_COUNT.store(read(CURRENT), Ordering::Relaxed);
            return ticks;
        }
        10 => return TICKS.load(Ordering::Relaxed),
        11 => return u64::from(HZ.load(Ordering::Acquire)),
        _ => {}
    }
    let _owner = RuntimeGuard::acquire();
    match operation {
        12 => u64::from(init_timer(b)),
        13 => u64::from(one_shot(state())),
        14 => arm(a),
        15 => {
            disarm(state());
            0
        }
        16 => u64::from(resume(state())),
        17 => {
            publish(stop_value(state()));
            0
        }
        18 => {
            let value = state();
            u64::from(
                value.has(TIMER_ENABLED) && value.has(CALIBRATED) && value.timer_initial_count != 0,
            )
        }
        19 => u64::from(if BASE.load(Ordering::Acquire) != 0 {
            read(CURRENT)
        } else {
            state().timer_current_count
        }),
        _ => panic!("Invalid private LAPIC operation"),
    }
}
/// # Safety
/// Caller owns one aligned writable fixed C output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_lapic_status(output: *mut Status) {
    let _owner = RuntimeGuard::acquire();
    unsafe { output.write(state()) };
}

// Shared native presentation of the existing fixed diagnostic codes.
pub fn timer_reason_name(code: u8) -> &'static [u8] {
    match code {
        0 => b"timer not initialized",
        1 => b"LAPIC not enabled",
        2 => b"HPET reference clock unavailable",
        3 => b"HPET reference too slow",
        4 => b"LAPIC timer did not count during calibration",
        5 => b"LAPIC calibration denominator is zero",
        6 => b"LAPIC calibrated count out of range",
        7 => b"calibrated against HPET",
        8 => b"periodic LAPIC timer active",
        9 => b"LAPIC one-shot unavailable without calibration",
        10 => b"LAPIC one-shot ready",
        11 => b"LAPIC one-shot armed",
        12 => b"LAPIC one-shot disarmed",
        13 => b"LAPIC periodic resume unavailable",
        14 => b"qualified TSC-deadline timer active",
        15 => b"TSC-deadline revoked; calibrated count timer retained",
        _ => panic!("Invalid lapic diagnostic code"),
    }
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 88);
    assert!(core::mem::offset_of!(Status, flags) == 40);
    assert!(core::mem::offset_of!(Status, timer_reason) == 80);
};

unsafe fn lapic_log(event: u8, status: *const Status) {
    use crate::boot::log::{
        r4os_rust_bootlog_dec as dec, r4os_rust_bootlog_hex as hex, text as puts,
    };
    let s = unsafe { status.read() };

    match event {
        0 => {
            let mode: &[u8] = if s.has(X2APIC) {
                b"x2apic"
            } else if s.virt_base != 0 {
                b"xapic"
            } else {
                b"none"
            };
            crate::boot::log_route::puts(b"[LAPIC] mode=");
            crate::boot::log_route::puts(mode);
            crate::boot::log_route::puts(b"\r\n");
            puts(b"[LAPIC] mode=");
            puts(mode);
            puts(b" ");
            puts(if s.has(AVAILABLE) {
                b"available"
            } else {
                b"missing"
            });
            puts(b" enabled=");
            puts(if s.has(ENABLED) && s.has(SOFTWARE) {
                b"yes"
            } else {
                b"no"
            });
            puts(b" base=0x");
            hex(s.phys_base, 16);
            puts(b" id=");
            dec(u64::from(s.id));
            puts(b" ver=0x");
            hex(u64::from(s.version), 2);
            puts(b" reason=");
            puts(match s.reason {
                0 => b"not initialized",
                1 => b"MADT has no enabled LAPIC",
                2 => b"x2APIC MSR mode active, PIC/IOAPIC routing unchanged",
                3 => b"LAPIC base missing",
                4 => b"LAPIC MMIO mapping failed",
                5 => b"xAPIC MMIO enabled, PIC/IOAPIC routing unchanged",
                _ => unreachable!(),
            });
            puts(b"\r\n");
        }
        1 | 2 => {
            puts(b"[LAPIC] timer=");
            puts(if event == 2 { b"active" } else { b"off" });
            puts(b" hz=");
            dec(u64::from(s.timer_frequency_hz));
            puts(b" initial=");
            dec(u64::from(s.timer_initial_count));
            puts(b" hpet_ticks=");
            dec(s.calibration_hpet_ticks);
            puts(b" lapic_ticks=");
            dec(u64::from(s.calibration_lapic_ticks));
            puts(b" reason=");
            puts(match s.timer_reason {
                0 => b"timer not initialized",
                1 => b"LAPIC not enabled",
                2 => b"HPET reference clock unavailable",
                3 => b"HPET reference too slow",
                4 => b"LAPIC timer did not count during calibration",
                5 => b"LAPIC calibration denominator is zero",
                6 => b"LAPIC calibrated count out of range",
                7 => b"calibrated against HPET",
                8 => b"periodic LAPIC timer active",
                9 => b"LAPIC one-shot unavailable without calibration",
                10 => b"LAPIC one-shot ready",
                11 => b"LAPIC one-shot armed",
                12 => b"LAPIC one-shot disarmed",
                13 => b"LAPIC periodic resume unavailable",
                _ => unreachable!(),
            });
            puts(b"\r\n");
        }
        _ => unreachable!(),
    }
}
