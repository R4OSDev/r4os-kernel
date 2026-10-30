//! Single HPET controller, admitted by BSP boot or the existing runtime owner.
//! Counter reads are IRQ-independent; no State reference survives mapping/logging.
use super::runtime::RuntimeGuard;
use crate::{
    memory::{paging, phys},
    platform::{
        clock_math as math,
        timer_arm::{self, Revision, Stamp},
    },
};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};
const AVAILABLE: u32 = 1;
const MAPPED: u32 = 2;
const ENABLED: u32 = 4;
const LEGACY: u32 = 8;
const COUNTER64: u32 = 16;
const PERIODIC_CAP: u32 = 32;
const TIMER64: u32 = 64;
const IRQ_ACTIVE: u32 = 128;
const ONE_SHOT: u32 = 256;
const INT_LEVEL: u64 = 2;
const INT_ENABLE: u64 = 4;
const PERIODIC: u64 = 8;
const SET_ACCUMULATOR: u64 = 64;
const TIMER32: u64 = 256;
const ROUTE: u64 = 0x1f << 9;
const FSB: u64 = 1 << 14;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Status {
    pub flags: u32,
    pub comparator_count: u8,
    pub revision: u8,
    pub number: u8,
    pub reason: u8,
    pub min_tick: u16,
    pub vendor_id: u16,
    pub period_fs: u32,
    pub frequency_hz: u64,
    pub phys_base: u64,
    pub virt_base: u64,
    pub config: u64,
    pub timer0_config: u64,
    pub timer0_comparator: u64,
    pub timer0_period: u64,
    pub counter: u64,
    pub timer_ticks: u64,
    pub timer_hz: u32,
}
impl Status {
    const EMPTY: Self = Self {
        flags: 0,
        comparator_count: 0,
        revision: 0,
        number: 0,
        reason: 0,
        min_tick: 0,
        vendor_id: 0,
        period_fs: 0,
        frequency_hz: 0,
        phys_base: 0,
        virt_base: 0,
        config: 0,
        timer0_config: 0,
        timer0_comparator: 0,
        timer0_period: 0,
        counter: 0,
        timer_ticks: 0,
        timer_hz: 0,
    };
    fn has(self, flag: u32) -> bool {
        self.flags & flag != 0
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
// SAFETY: every mutable state access is covered by the original runtime owner.
// Lock-free counters/base/width live separately; snapshots never alias writes.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(Status::EMPTY));
static BASE: AtomicU64 = AtomicU64::new(0);
static WIDTH: AtomicU32 = AtomicU32::new(0);
static EXTENSION: AtomicU64 = AtomicU64::new(0);
static TICKS: AtomicU64 = AtomicU64::new(0);
static HZ: AtomicU32 = AtomicU32::new(0);
static ARM_REVISION: Revision = Revision::new();

pub(crate) fn idle_arm_stamp() -> Option<Stamp> {
    ARM_REVISION.stamp()
}
pub(crate) fn idle_counter_wide() -> bool {
    state().has(COUNTER64)
}
/// Existing Runtime/BSP owner retains all comparator and mapping writes.
/// Hardware counter/pending observations additionally reject consumed arms.
pub(crate) fn retains_idle_arm(stamp: Stamp, finite: bool) -> bool {
    let value = state();
    if !ARM_REVISION.matches(stamp)
        || !value.has(MAPPED)
        || !value.has(ONE_SHOT)
        || read(0x10) & 3 != 3
    {
        return false;
    }
    let config = read(0x100);
    if config & PERIODIC != 0 || (config & INT_ENABLE != 0) != finite || read(0x20) & 1 != 0 {
        return false;
    }
    let valid = if finite {
        // A narrow main counter can alias an entire elapsed wrap. Until its
        // separate long-idle policy can prove elapsed time, keep programming
        // normally. A narrow comparator over a wide counter remains bounded.
        if !value.has(COUNTER64) {
            return false;
        }
        let (mask, maximum) = if value.has(TIMER64) {
            (u64::MAX, 0x3fff_ffff_ffff_ffff)
        } else {
            (u64::from(u32::MAX), 0x7fff_ffff)
        };
        let target = read(0x108) & mask;
        timer_arm::counter_future(main_counter(), target, mask, maximum)
    } else {
        true
    };
    valid && ARM_REVISION.matches(stamp)
}

fn state() -> Status {
    unsafe { *STATE.0.get() }
}
fn publish(value: Status) {
    unsafe { STATE.0.get().write(value) };
    HZ.store(value.timer_hz, Ordering::Release);
}
fn reg(offset: u64) -> *mut u64 {
    core::ptr::with_exposed_provenance_mut((BASE.load(Ordering::Acquire) + offset) as usize)
}
fn read(offset: u64) -> u64 {
    unsafe { reg(offset).read_volatile() }
}
fn write(offset: u64, value: u64) {
    ARM_REVISION.advance_owned();
    unsafe { reg(offset).write_volatile(value) };
}
fn log(mut value: Status) {
    value.timer_ticks = TICKS.load(Ordering::Relaxed);
    unsafe { hpet_log(&value) };
}
fn map(base: u64) -> bool {
    let page = base & !4095;
    let virtual_page = phys::r4os_rust_phys_to_virt(page);
    if !paging::ensure_mmio(virtual_page, page) {
        return false;
    }
    ARM_REVISION.advance_owned();
    BASE.store(phys::r4os_rust_phys_to_virt(base), Ordering::Release);
    true
}
pub fn main_counter() -> u64 {
    if BASE.load(Ordering::Acquire) == 0 {
        return 0;
    }
    let value = read(0xf0);
    if WIDTH.load(Ordering::Acquire) != 0 {
        value
    } else {
        value & u64::from(u32::MAX)
    }
}
pub fn extended_counter() -> u64 {
    if BASE.load(Ordering::Acquire) == 0 {
        return 0;
    }
    if WIDTH.load(Ordering::Acquire) != 0 {
        return main_counter();
    }
    let mut published = EXTENSION.load(Ordering::Acquire);
    loop {
        let low = main_counter() as u32;
        let next = math::extend_counter32(published, low);
        if next == published {
            return published;
        }
        match EXTENSION.compare_exchange_weak(published, next, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => return next,
            Err(actual) => published = actual,
        }
    }
}
pub fn elapsed(start: u64, end: u64) -> u64 {
    let delta = end.wrapping_sub(start);
    if WIDTH.load(Ordering::Acquire) != 0 {
        delta
    } else {
        delta & u64::from(u32::MAX)
    }
}
fn advances(value: Status) -> bool {
    if !value.has(MAPPED) || !value.has(ENABLED) || value.frequency_hz == 0 {
        return false;
    }
    let start = main_counter();
    let half = if value.has(COUNTER64) {
        0x7fff_ffff_ffff_ffff
    } else {
        0x7fff_ffff
    };
    for _ in 0..65536 {
        let delta = elapsed(start, main_counter());
        if delta != 0 && delta <= half {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}
fn disable(mut value: Status) -> Status {
    if value.has(MAPPED) {
        write(0x10, read(0x10) & !3);
        value.config = read(0x10);
    }
    value.flags &= !(ENABLED | IRQ_ACTIVE | ONE_SHOT);
    value.frequency_hz = 0;
    value.reason = 3;
    value
}
pub fn init(base: u64, number: u8, min_tick: u16, legacy: bool) -> Status {
    let _owner = RuntimeGuard::acquire();
    ARM_REVISION.advance_owned();
    let mut value = Status {
        flags: if base != 0 { AVAILABLE } else { 0 } | if legacy { LEGACY } else { 0 },
        number,
        min_tick,
        phys_base: base,
        reason: 1,
        ..Status::EMPTY
    };
    BASE.store(0, Ordering::Release);
    WIDTH.store(0, Ordering::Release);
    TICKS.store(0, Ordering::Release);
    if base == 0 {
        publish(value);
        log(value);
        return value;
    }
    if !map(base) {
        value.reason = 2;
        publish(value);
        log(value);
        return value;
    }
    value.flags |= MAPPED;
    value.virt_base = BASE.load(Ordering::Acquire);
    let cap = read(0);
    value.revision = cap as u8;
    value.comparator_count = (((cap >> 8) & 31) + 1) as u8;
    value.set(COUNTER64, cap & (1 << 13) != 0);
    value.set(LEGACY, cap & (1 << 15) != 0);
    WIDTH.store(u32::from(value.has(COUNTER64)), Ordering::Release);
    value.vendor_id = (cap >> 16) as u16;
    value.period_fs = (cap >> 32) as u32;
    if value.period_fs != 0 {
        value.frequency_hz = 1_000_000_000_000_000 / u64::from(value.period_fs);
    }
    write(0x10, (read(0x10) | 1) & !2);
    value.config = read(0x10);
    value.set(ENABLED, value.config & 1 != 0);
    if value.comparator_count > 0 {
        value.timer0_config = read(0x100);
        value.set(PERIODIC_CAP, value.timer0_config & 16 != 0);
        value.set(TIMER64, value.timer0_config & 32 != 0);
    }
    value.counter = main_counter();
    EXTENSION.store(value.counter & u64::from(u32::MAX), Ordering::Release);
    // LAPIC calibration/startup uses raw samples of at most 10 ms; event
    // handoff has a 50 ms raw timeout. Admit a narrow reference with room
    // for those complete intervals before its half-wrap ambiguity.
    if !value.has(COUNTER64)
        && value.frequency_hz != 0
        && math::counter32_observation_ticks(
            value.frequency_hz,
            math::Rate {
                numerator: 20,
                denominator: 1,
            },
        )
        .is_none()
    {
        value = disable(value);
        value.reason = 17;
        publish(value);
        log(value);
        return value;
    }
    if advances(value) {
        value.reason = 4;
    } else {
        value = disable(value);
    }
    publish(value);
    log(value);
    value
}
pub fn snapshot() -> Status {
    let _owner = RuntimeGuard::acquire();
    let mut value = state();
    value.timer_ticks = TICKS.load(Ordering::Relaxed);
    if value.has(MAPPED) {
        value.config = read(0x10);
        value.set(ENABLED, value.config & 1 != 0);
        value.counter = main_counter();
        if value.comparator_count > 0 {
            value.timer0_config = read(0x100);
            value.timer0_comparator = read(0x108);
        }
    }
    value
}
fn periodic(hz: u32, log_status: bool) -> bool {
    let mut value = state();
    if !value.has(MAPPED) || value.frequency_hz == 0 || value.comparator_count == 0 {
        value.reason = 5;
        publish(value);
        return false;
    }
    if !value.has(LEGACY) {
        value.reason = 6;
        publish(value);
        return false;
    }
    if !value.has(PERIODIC_CAP) {
        value.reason = 7;
        publish(value);
        return false;
    }
    let hz = if hz == 0 { 100 } else { hz };
    let period = (value.frequency_hz / u64::from(hz)).max(1);
    value.timer_hz = hz;
    value.timer0_period = period;
    value.flags &= !ONE_SHOT;
    let mut config = read(0x100) & !(INT_LEVEL | INT_ENABLE | PERIODIC | TIMER32 | ROUTE | FSB);
    config |= PERIODIC | SET_ACCUMULATOR;
    if value.has(TIMER64) && !value.has(COUNTER64) {
        config |= TIMER32;
    }
    write(0x100, config);
    let mask = if value.has(COUNTER64) && value.has(TIMER64) {
        u64::MAX
    } else {
        u64::from(u32::MAX)
    };
    write(0x108, main_counter().wrapping_add(period) & mask);
    write(0x108, period);
    config |= INT_ENABLE;
    write(0x100, config);
    write(0x10, read(0x10) | 3);
    value.config = read(0x10);
    value.timer0_config = read(0x100);
    value.timer0_comparator = read(0x108);
    value.counter = main_counter();
    value.set(ENABLED, value.config & 1 != 0);
    value.set(
        IRQ_ACTIVE,
        value.has(ENABLED) && value.config & 2 != 0 && value.timer0_config & INT_ENABLE != 0,
    );
    value.reason = if value.has(IRQ_ACTIVE) { 8 } else { 9 };
    publish(value);
    if log_status {
        log(value);
    }
    value.has(IRQ_ACTIVE)
}
fn one_shot(hz: u32) -> bool {
    let mut value = state();
    if !value.has(MAPPED) || value.frequency_hz == 0 || value.comparator_count == 0 {
        value.reason = 10;
        publish(value);
        return false;
    }
    if !value.has(LEGACY) {
        value.reason = 11;
        publish(value);
        return false;
    }
    write(
        0x100,
        read(0x100)
            & !(INT_LEVEL | INT_ENABLE | PERIODIC | SET_ACCUMULATOR | TIMER32 | ROUTE | FSB),
    );
    write(0x20, 1);
    write(0x10, read(0x10) | 3);
    value.config = read(0x10);
    value.timer0_config = read(0x100);
    value.set(ENABLED, value.config & 1 != 0);
    value.flags &= !IRQ_ACTIVE;
    value.set(ONE_SHOT, value.has(ENABLED) && value.config & 2 != 0);
    value.timer_hz = hz;
    value.timer0_period = 0;
    value.reason = if value.has(ONE_SHOT) { 12 } else { 13 };
    publish(value);
    value.has(ONE_SHOT)
}
fn arm(ticks: u64, hz: u32) -> u64 {
    if !state().has(ONE_SHOT) && !one_shot(hz) {
        return 0;
    }
    let mut value = state();
    if hz == 0 || value.frequency_hz == 0 {
        return 0;
    }
    let wanted = ticks.max(1);
    let limit = if value.has(COUNTER64) && value.has(TIMER64) {
        0x3fff_ffff_ffff_ffff
    } else {
        0x7fff_ffff
    };
    let mut delta = math::scale_ceil(wanted, value.frequency_hz, u64::from(hz))
        .max(u64::from(value.min_tick.max(1)));
    let clamped = delta > limit;
    if clamped {
        delta = limit;
    }
    let mut config = read(0x100)
        & !(INT_LEVEL | INT_ENABLE | PERIODIC | SET_ACCUMULATOR | TIMER32 | ROUTE | FSB);
    if value.has(TIMER64) && !value.has(COUNTER64) {
        config |= TIMER32;
    }
    write(0x100, config);
    write(0x20, 1);
    let mask = if value.has(COUNTER64) && value.has(TIMER64) {
        u64::MAX
    } else {
        u64::from(u32::MAX)
    };
    let comparator = main_counter().wrapping_add(delta) & mask;
    write(0x108, comparator);
    config |= INT_ENABLE;
    write(0x100, config);
    value.timer0_config = read(0x100);
    value.timer0_comparator = comparator;
    value.set(IRQ_ACTIVE, value.timer0_config & INT_ENABLE != 0);
    publish(value);
    if !value.has(IRQ_ACTIVE) {
        return 0;
    }
    value.reason = 14;
    publish(value);
    if clamped {
        math::scale_floor(delta, u64::from(hz), value.frequency_hz).max(1)
    } else {
        wanted
    }
}
fn disarm() {
    let mut value = state();
    if !value.has(MAPPED) || !value.has(ONE_SHOT) {
        return;
    }
    write(0x100, read(0x100) & !INT_ENABLE);
    write(0x20, 1);
    value.timer0_config = read(0x100);
    value.flags &= !IRQ_ACTIVE;
    value.reason = 15;
    publish(value);
}
fn stop() {
    let mut value = state();
    if !value.has(MAPPED) {
        return;
    }
    write(
        0x100,
        read(0x100) & !(INT_ENABLE | PERIODIC | SET_ACCUMULATOR),
    );
    write(0x10, (read(0x10) & !2) | 1);
    value.config = read(0x10);
    value.timer0_config = read(0x100);
    value.timer0_comparator = read(0x108);
    value.set(ENABLED, value.config & 1 != 0);
    value.flags &= !(IRQ_ACTIVE | ONE_SHOT);
    value.timer_hz = 0;
    value.timer0_period = 0;
    value.reason = 16;
    publish(value);
    log(value);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_hpet_control(operation: u8, a: u64, b: u32) -> u64 {
    match operation {
        0 => return main_counter(),
        1 => return extended_counter(),
        3 => {
            if BASE.load(Ordering::Acquire) != 0 {
                write(0x20, 1);
            }
            return TICKS.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        }
        4 => return TICKS.load(Ordering::Relaxed),
        5 => return u64::from(HZ.load(Ordering::Acquire)),
        _ => {}
    }
    let _owner = RuntimeGuard::acquire();
    match operation {
        6 => u64::from(periodic(b, true)),
        7 => u64::from(periodic(b, false)),
        8 => u64::from(one_shot(b)),
        9 => arm(a, b),
        10 => {
            disarm();
            0
        }
        11 => {
            stop();
            0
        }
        12 => {
            publish(disable(state()));
            0
        }
        _ => panic!("Invalid private HPET operation"),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_hpet_elapsed(start: u64, end: u64) -> u64 {
    elapsed(start, end)
}
/// # Safety
/// Singleton BSP startup; caller owns an aligned writable fixed C output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_hpet_init(
    base: u64,
    number: u8,
    min_tick: u16,
    legacy: bool,
    output: *mut Status,
) {
    unsafe { output.write(init(base, number, min_tick, legacy)) };
}
/// # Safety
/// Caller owns one aligned writable C snapshot until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_hpet_status(output: *mut Status) {
    unsafe { output.write(snapshot()) };
}

// Shared native presentation of the existing fixed diagnostic codes.
pub fn reason_name(code: u8) -> &'static [u8] {
    match code {
        0 => b"not initialized",
        1 => b"ACPI HPET missing",
        2 => b"HPET MMIO mapping failed",
        3 => b"HPET main counter did not advance within bounded qualification",
        4 => b"MMIO mapped, main counter enabled, interrupts disabled",
        5 => b"HPET timer unavailable",
        6 => b"HPET legacy replacement not supported",
        7 => b"HPET comparator 0 is not periodic capable",
        8 => b"HPET comparator 0 periodic legacy IRQ0 active",
        9 => b"HPET comparator 0 setup incomplete",
        10 => b"HPET one-shot unavailable",
        11 => b"HPET one-shot needs legacy IRQ0 replacement",
        12 => b"HPET comparator 0 one-shot legacy IRQ0 ready",
        13 => b"HPET one-shot setup incomplete",
        14 => b"HPET comparator 0 one-shot armed",
        15 => b"HPET comparator 0 one-shot disarmed",
        16 => b"HPET main counter enabled, comparator interrupts disabled",
        17 => b"32-bit HPET cannot qualify bounded reference intervals",
        _ => panic!("Invalid hpet diagnostic code"),
    }
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 96);
    assert!(core::mem::offset_of!(Status, timer_hz) == 88);
};

unsafe fn hpet_log(status: *const Status) {
    use crate::boot::log::{
        r4os_rust_bootlog_dec as dec, r4os_rust_bootlog_hex as hex, text as puts,
    };
    let s = unsafe { status.read() };

    puts(b"[HPET] ");
    puts(if s.has(AVAILABLE) {
        b"available"
    } else {
        b"missing"
    });
    puts(b" mapped=");
    puts(if s.has(MAPPED) { b"yes" } else { b"no" });
    puts(b" enabled=");
    puts(if s.has(ENABLED) { b"yes" } else { b"no" });
    puts(b" base=0x");
    hex(s.phys_base, 16);
    puts(b" period_fs=");
    dec(u64::from(s.period_fs));
    puts(b" freq_hz=");
    dec(s.frequency_hz);
    puts(b" counter=");
    dec(s.counter);
    puts(b" timer0_irq=");
    puts(if s.flags & 128 != 0 { b"on" } else { b"off" });
    puts(b" timer_hz=");
    dec(u64::from(s.timer_hz));
    puts(b" timer_ticks=");
    dec(s.timer_ticks);
    puts(b" reason=");
    puts(match s.reason {
        0 => b"not initialized",
        1 => b"ACPI HPET missing",
        2 => b"HPET MMIO mapping failed",
        3 => b"HPET main counter did not advance within bounded qualification",
        4 => b"MMIO mapped, main counter enabled, interrupts disabled",
        5 => b"HPET timer unavailable",
        6 => b"HPET legacy replacement not supported",
        7 => b"HPET comparator 0 is not periodic capable",
        8 => b"HPET comparator 0 periodic legacy IRQ0 active",
        9 => b"HPET comparator 0 setup incomplete",
        10 => b"HPET one-shot unavailable",
        11 => b"HPET one-shot needs legacy IRQ0 replacement",
        12 => b"HPET comparator 0 one-shot legacy IRQ0 ready",
        13 => b"HPET one-shot setup incomplete",
        14 => b"HPET comparator 0 one-shot armed",
        15 => b"HPET comparator 0 one-shot disarmed",
        16 => b"HPET main counter enabled, comparator interrupts disabled",
        17 => b"32-bit HPET cannot qualify bounded reference intervals",
        _ => unreachable!(),
    });
    puts(b"\r\n");
}
