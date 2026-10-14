//! Single scheduler/wait/IRQ runtime owner. Context switch releases the outer
//! token after outgoing RSP is durable, below the incoming saved context.
//! Unrelated owners never borrow this admission.
use crate::{build_bridge::config, memory::owner};
use core::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
const IF: u64 = 1 << 9;
const MAX_CPUS: usize = 32;
static ENABLED: AtomicBool = AtomicBool::new(false);
static LOCK: AtomicU8 = AtomicU8::new(0);
static OWNER_CPU_PLUS_ONE: AtomicU8 = AtomicU8::new(0);
static ACQUIRED_TSC: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static ACQUISITIONS: AtomicU64 = AtomicU64::new(0);
static NESTED: AtomicU64 = AtomicU64::new(0);
static COLLISIONS: AtomicU64 = AtomicU64::new(0);
static CPU_COLLISIONS: AtomicU64 = AtomicU64::new(0);
static WAIT_SPINS: AtomicU64 = AtomicU64::new(0);
static MAX_WAIT_SPINS: AtomicU64 = AtomicU64::new(0);
static HOLD_CYCLES: AtomicU64 = AtomicU64::new(0);
static MAX_HOLD_CYCLES: AtomicU64 = AtomicU64::new(0);
#[repr(C)]
#[derive(Default)]
pub struct Stats {
    pub acquisitions: u64,
    pub nested_acquisitions: u64,
    pub collisions: u64,
    pub cpu_collisions: u64,
    pub wait_spins: u64,
    pub max_wait_spins: u64,
    pub hold_cycles: u64,
    pub max_hold_cycles: u64,
    pub legacy_global_acquisitions: u64,
}
pub fn current_cpu() -> u32 {
    super::percpu::current_index()
}
fn depth() -> *mut u32 {
    super::percpu::r4os_rust_percpu_depth()
}
fn flags() -> u64 {
    let value;
    unsafe { core::arch::asm!("pushfq", "pop {}", out(reg) value, options(preserves_flags)) };
    value
}
fn cli() {
    unsafe { core::arch::asm!("cli", options(nostack)) };
}
fn sti() {
    unsafe { core::arch::asm!("sti", options(nostack)) };
}
fn hlt() {
    unsafe { core::arch::asm!("hlt", options(nostack)) };
}
fn tsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("rdtsc", out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags))
    };
    (u64::from(high) << 32) | u64::from(low)
}
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}
fn restore_local(saved: u64) {
    if saved & IF != 0 {
        sti();
    } else {
        cli();
    }
}
fn save_local() -> u64 {
    let saved = flags();
    cli();
    saved
}
fn acquire(saved: u64) {
    if !enabled() {
        return;
    }
    let cpu = current_cpu();
    let slot = cpu as usize;
    let depth = depth();
    ACQUISITIONS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: this raw slot belongs to the current CPU with IF=0. TLB IPIs
    // may run while contending but never mutate runtime depth or switch tasks.
    // No Rust reference survives a callback, IRQ or eventual context switch.
    let previous = unsafe { depth.read() };
    if previous != 0 {
        unsafe { depth.write(previous.saturating_add(1)) };
        NESTED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let mut spins = 0u64;
    let mut collided = false;
    let mut cpu_collision = false;
    while LOCK
        .compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        collided = true;
        if config::ENABLE_METRICS {
            spins = spins.saturating_add(1);
        }
        let holder = OWNER_CPU_PLUS_ONE.load(Ordering::Acquire);
        let this_cpu = u8::try_from(cpu + 1).expect("Runtime CPU identity exceeds owner field");
        if holder != 0 && holder != this_cpu {
            cpu_collision = true;
        }
        if saved & IF != 0 {
            sti();
            core::hint::spin_loop();
            cli();
        } else {
            core::hint::spin_loop();
        }
    }
    if collided {
        COLLISIONS.fetch_add(1, Ordering::Relaxed);
    }
    if cpu_collision {
        CPU_COLLISIONS.fetch_add(1, Ordering::Relaxed);
    }
    if config::ENABLE_METRICS && collided {
        WAIT_SPINS.fetch_add(spins, Ordering::Relaxed);
        MAX_WAIT_SPINS.fetch_max(spins, Ordering::Relaxed);
    }
    OWNER_CPU_PLUS_ONE.store(
        u8::try_from(cpu + 1).expect("Runtime CPU identity exceeds owner field"),
        Ordering::Release,
    );
    if config::ENABLE_METRICS {
        ACQUIRED_TSC[slot].store(tsc(), Ordering::Relaxed);
    }
    unsafe { depth.write(1) };
}
fn finish_outer(cpu: u32) {
    if config::ENABLE_METRICS {
        let cycles = tsc().wrapping_sub(ACQUIRED_TSC[cpu as usize].load(Ordering::Relaxed));
        HOLD_CYCLES.fetch_add(cycles, Ordering::Relaxed);
        MAX_HOLD_CYCLES.fetch_max(cycles, Ordering::Relaxed);
    }
    OWNER_CPU_PLUS_ONE.store(0, Ordering::Release);
}
fn release() {
    if !enabled() {
        return;
    }
    let depth = depth();
    let previous = unsafe { depth.read() };
    if previous == 0 {
        return;
    }
    unsafe { depth.write(previous - 1) };
    if previous == 1 {
        finish_outer(current_cpu());
        LOCK.store(0, Ordering::Release);
    }
}
pub fn save_runtime() -> u64 {
    let saved = save_local();
    owner::prepare_stack();
    acquire(saved);
    saved
}
pub fn restore_runtime(saved: u64) {
    release();
    restore_local(saved);
}
fn enable_runtime() {
    for value in &ACQUIRED_TSC {
        value.store(0, Ordering::Relaxed);
    }
    for value in [
        &ACQUISITIONS,
        &NESTED,
        &COLLISIONS,
        &CPU_COLLISIONS,
        &WAIT_SPINS,
        &MAX_WAIT_SPINS,
        &HOLD_CYCLES,
        &MAX_HOLD_CYCLES,
    ] {
        value.store(0, Ordering::Relaxed);
    }
    OWNER_CPU_PLUS_ONE.store(0, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Release);
}
pub fn release_for_switch() -> bool {
    if !enabled() {
        return true;
    }
    let depth = depth();
    if unsafe { depth.read() } != 1 {
        return false;
    }
    unsafe { depth.write(0) };
    finish_outer(current_cpu());
    LOCK.store(0, Ordering::Release);
    true
}
pub fn halt_forever() -> ! {
    cli();
    loop {
        hlt();
    }
}
pub struct RuntimeGuard {
    flags: u64,
    _cpu_bound: PhantomData<*mut ()>,
}
impl RuntimeGuard {
    #[cfg(not(r4os_owner_fixture))]
    pub fn acquire() -> Self {
        Self {
            flags: save_runtime(),
            _cpu_bound: PhantomData,
        }
    }
    // Serialized host metadata fixtures never execute privileged hardware.
    #[cfg(r4os_owner_fixture)]
    pub fn acquire() -> Self {
        Self {
            flags: 0,
            _cpu_bound: PhantomData,
        }
    }
}
impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        let _ = self.flags;
        #[cfg(not(r4os_owner_fixture))]
        restore_runtime(self.flags);
    }
}
/// Private CPL0 operations. Callers retain the original IRQ/CPU/boot context;
/// operation 9 requires IF=0 and exactly one token before the stack switch.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_runtime_control(operation: u8, saved: u64) -> u64 {
    match operation {
        0 => {
            cli();
            0
        }
        1 => {
            sti();
            0
        }
        2 => save_local(),
        3 => {
            restore_local(saved);
            0
        }
        4 => save_runtime(),
        5 => {
            restore_runtime(saved);
            0
        }
        6 => {
            enable_runtime();
            0
        }
        7 => u64::from(enabled()),
        8 => u64::from(enabled() && unsafe { depth().read() } != 0),
        9 => u64::from(release_for_switch()),
        10 => u64::from(saved & IF != 0),
        11 => halt_forever(),
        12 => {
            hlt();
            0
        }
        13 => {
            unsafe { core::arch::asm!("sti; hlt", options(nostack)) };
            0
        }
        _ => panic!("Invalid private runtime operation"),
    }
}
/// # Safety
/// Caller owns one aligned writable fixed C output for the passive snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_runtime_stats(output: *mut Stats) {
    let value = Stats {
        acquisitions: ACQUISITIONS.load(Ordering::Relaxed),
        nested_acquisitions: NESTED.load(Ordering::Relaxed),
        collisions: COLLISIONS.load(Ordering::Relaxed),
        cpu_collisions: CPU_COLLISIONS.load(Ordering::Relaxed),
        wait_spins: WAIT_SPINS.load(Ordering::Relaxed),
        max_wait_spins: MAX_WAIT_SPINS.load(Ordering::Relaxed),
        hold_cycles: HOLD_CYCLES.load(Ordering::Relaxed),
        max_hold_cycles: MAX_HOLD_CYCLES.load(Ordering::Relaxed),
        legacy_global_acquisitions: 0,
    };
    unsafe { output.write(value) };
}
const _: () = {
    assert!(core::mem::size_of::<Stats>() == 72);
    assert!(core::mem::offset_of!(Stats, legacy_global_acquisitions) == 64);
};
