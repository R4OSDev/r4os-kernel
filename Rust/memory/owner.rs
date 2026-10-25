//! Ranked, CPU-bound no-sleep owners. Resident C-layout storage can be
//! embedded in stable native records; only its owner mutates the lock state.
use super::{layout, owner_policy};
use crate::arch::smp_policy::MAX_CPUS;
use crate::build_config::config;
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};
const IF: u64 = 1 << 9;
const STACK_HEADROOM: u64 = 64 * 1024;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Token {
    pub flags: u64,
    pub cpu_index: u32,
    pub previous_rank: u8,
    pub outermost: bool,
}
#[repr(C)]
#[derive(Default)]
pub struct Stats {
    pub acquisitions: u64,
    pub nested_acquisitions: u64,
    pub collisions: u64,
    pub wait_spins: u64,
    pub max_wait_spins: u64,
    pub hold_cycles: u64,
    pub max_hold_cycles: u64,
    pub order_violations: u64,
}
#[repr(C)]
pub struct Lock {
    class: u8,
    rank: u8,
    state: AtomicU8,
    owner_cpu_plus_one: AtomicU8,
    depth: [UnsafeCell<u16>; MAX_CPUS],
    acquired_tsc: [UnsafeCell<u64>; MAX_CPUS],
    acquisitions: AtomicU64,
    nested_acquisitions: AtomicU64,
    collisions: AtomicU64,
    wait_spins: AtomicU64,
    max_wait_spins: AtomicU64,
    hold_cycles: AtomicU64,
    max_hold_cycles: AtomicU64,
    order_violations: AtomicU64,
}
// SAFETY: depth/TSC slots belong to the exact CPU while IRQs are disabled;
// admission and observer counters are atomic. No exclusive Lock reference is
// created, including reentrant acquisitions or concurrent metric snapshots.
unsafe impl Sync for Lock {}
// Original named owner storage; one native instance per existing class.
static NAMED: [Lock; 12] = [
    Lock::new(0, 1),
    Lock::new(1, 2),
    Lock::new(2, 2),
    Lock::new(3, 2),
    Lock::new(4, 5),
    Lock::new(5, 10),
    Lock::new(6, 20),
    Lock::new(7, 30),
    Lock::new(8, 70),
    Lock::new(9, 80),
    // Last leaf: only paired PCI configuration port instructions, never a
    // callback, allocation, mapping operation or acquisition of another owner.
    Lock::new(10, 85),
    // RTC leaf: bounded CMOS/calendar/cache operations, no wait or peer owner.
    Lock::new(11, 86),
];
static HELD_RANK: [AtomicU8; MAX_CPUS] = [const { AtomicU8::new(0) }; MAX_CPUS];

fn cpu() -> usize {
    ({ crate::arch::percpu::current_index() }) as usize
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
fn pause() {
    core::hint::spin_loop();
}
fn restore(value: u64) {
    if value & IF != 0 {
        sti();
    } else {
        cli();
    }
}
fn tsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("rdtsc", out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags))
    };
    (high as u64) << 32 | low as u64
}
pub fn fault_allowed() -> bool {
    HELD_RANK[cpu()].load(Ordering::Relaxed) == 0 && unsafe { owner_runtime_depth() } == 0
}
pub fn prepare_stack() {
    if !fault_allowed() {
        return;
    }
    let rsp: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags))
    };
    if !(layout::APP_STACK_BASE..layout::APP_STACK_BASE + layout::APP_STACK_BYTES).contains(&rsp) {
        return;
    }
    let mut offset = layout::PAGE_SIZE;
    while offset <= STACK_HEADROOM {
        unsafe {
            core::ptr::with_exposed_provenance::<u8>((rsp - offset) as usize).read_volatile()
        };
        offset += layout::PAGE_SIZE;
    }
    // A synchronous stack-growth fault may have changed resident metadata.
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
}
impl Lock {
    /// Native owner storage, matching the original zeroed C lock initializer.
    pub(crate) const fn new(class: u8, rank: u8) -> Self {
        Self {
            class,
            rank,
            state: AtomicU8::new(0),
            owner_cpu_plus_one: AtomicU8::new(0),
            depth: [const { UnsafeCell::new(0) }; MAX_CPUS],
            acquired_tsc: [const { UnsafeCell::new(0) }; MAX_CPUS],
            acquisitions: AtomicU64::new(0),
            nested_acquisitions: AtomicU64::new(0),
            collisions: AtomicU64::new(0),
            wait_spins: AtomicU64::new(0),
            max_wait_spins: AtomicU64::new(0),
            hold_cycles: AtomicU64::new(0),
            max_hold_cycles: AtomicU64::new(0),
            order_violations: AtomicU64::new(0),
        }
    }

    pub fn acquire(&self) -> Token {
        let saved = flags();
        cli();
        prepare_stack();
        let slot = cpu();
        let owner = (slot + 1) as u8;
        let previous = HELD_RANK[slot].load(Ordering::Relaxed);
        let depth = unsafe { self.depth[slot].get().read() };
        if self.owner_cpu_plus_one.load(Ordering::Acquire) == owner && depth != 0 {
            unsafe { self.depth[slot].get().write(depth.saturating_add(1)) };
            self.nested_acquisitions.fetch_add(1, Ordering::Relaxed);
            self.acquisitions.fetch_add(1, Ordering::Relaxed);
            return Token {
                flags: saved,
                cpu_index: slot as u32,
                previous_rank: previous,
                outermost: false,
            };
        }
        if !owner_policy::order_allowed(previous, self.rank) {
            self.order_violations.fetch_add(1, Ordering::Relaxed);
        }
        let mut spins: u64 = 0;
        let mut collided = false;
        while self
            .state
            .compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            collided = true;
            loop {
                if config::ENABLE_METRICS {
                    spins = spins.saturating_add(1);
                }
                // IF=1 callers service ordinary IPIs. Fault/IRQ IF=0 callers
                // must also acknowledge a pending TLB request: its sender may
                // hold this owner. They never borrow enabled interrupts.
                if saved & IF != 0 {
                    sti();
                    pause();
                    cli();
                } else {
                    crate::arch::tlb::service_pending();
                    pause();
                }
                // Observation is only a retry hint. Ownership still requires
                // the successful Acquire CAS above, including after an IRQ.
                if self.state.load(Ordering::Relaxed) == 0 {
                    break;
                }
            }
        }
        if collided {
            self.collisions.fetch_add(1, Ordering::Relaxed);
        }
        if config::ENABLE_METRICS && collided {
            self.wait_spins.fetch_add(spins, Ordering::Relaxed);
            self.max_wait_spins.fetch_max(spins, Ordering::Relaxed);
        }
        self.acquisitions.fetch_add(1, Ordering::Relaxed);
        self.owner_cpu_plus_one.store(owner, Ordering::Release);
        unsafe { self.depth[slot].get().write(1) };
        if config::ENABLE_METRICS {
            unsafe { self.acquired_tsc[slot].get().write(tsc()) };
        }
        HELD_RANK[slot].store(self.rank, Ordering::Relaxed);
        Token {
            flags: saved,
            cpu_index: slot as u32,
            previous_rank: previous,
            outermost: true,
        }
    }
    /// One bounded admission attempt. Refusal restores exactly the caller's
    /// IF and constructs no guard/token that could release a foreign owner.
    pub(crate) fn try_acquire(&self) -> Option<Token> {
        let saved = flags();
        cli();
        prepare_stack();
        let slot = cpu();
        let owner = (slot + 1) as u8;
        let previous = HELD_RANK[slot].load(Ordering::Relaxed);
        let depth = unsafe { self.depth[slot].get().read() };
        if self.owner_cpu_plus_one.load(Ordering::Acquire) == owner && depth != 0 {
            unsafe { self.depth[slot].get().write(depth.saturating_add(1)) };
            self.nested_acquisitions.fetch_add(1, Ordering::Relaxed);
            self.acquisitions.fetch_add(1, Ordering::Relaxed);
            return Some(Token {
                flags: saved,
                cpu_index: slot as u32,
                previous_rank: previous,
                outermost: false,
            });
        }
        if !owner_policy::order_allowed(previous, self.rank)
            || self
                .state
                .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
        {
            restore(saved);
            return None;
        }
        self.acquisitions.fetch_add(1, Ordering::Relaxed);
        self.owner_cpu_plus_one.store(owner, Ordering::Release);
        unsafe { self.depth[slot].get().write(1) };
        if config::ENABLE_METRICS {
            unsafe { self.acquired_tsc[slot].get().write(tsc()) };
        }
        HELD_RANK[slot].store(self.rank, Ordering::Relaxed);
        Some(Token {
            flags: saved,
            cpu_index: slot as u32,
            previous_rank: previous,
            outermost: true,
        })
    }
    pub fn release(&self, token: Token) {
        let slot = cpu();
        let owner = (slot + 1) as u8;
        let depth = unsafe { self.depth[slot].get().read() };
        if token.cpu_index != slot as u32
            || self.owner_cpu_plus_one.load(Ordering::Acquire) != owner
            || depth == 0
        {
            self.order_violations.fetch_add(1, Ordering::Relaxed);
            restore(token.flags);
            return;
        }
        unsafe { self.depth[slot].get().write(depth - 1) };
        if depth == 1 {
            if config::ENABLE_METRICS {
                let elapsed = tsc().wrapping_sub(unsafe { self.acquired_tsc[slot].get().read() });
                self.hold_cycles.fetch_add(elapsed, Ordering::Relaxed);
                self.max_hold_cycles.fetch_max(elapsed, Ordering::Relaxed);
            }
            HELD_RANK[slot].store(token.previous_rank, Ordering::Relaxed);
            self.owner_cpu_plus_one.store(0, Ordering::Release);
            self.state.store(0, Ordering::Release);
        }
        restore(token.flags);
    }
    pub fn held(&self) -> bool {
        let slot = cpu();
        self.owner_cpu_plus_one.load(Ordering::Acquire) == (slot + 1) as u8
            && unsafe { self.depth[slot].get().read() } != 0
    }
    pub fn stats(&self) -> Stats {
        Stats {
            acquisitions: self.acquisitions.load(Ordering::Relaxed),
            nested_acquisitions: self.nested_acquisitions.load(Ordering::Relaxed),
            collisions: self.collisions.load(Ordering::Relaxed),
            wait_spins: self.wait_spins.load(Ordering::Relaxed),
            max_wait_spins: self.max_wait_spins.load(Ordering::Relaxed),
            hold_cycles: self.hold_cycles.load(Ordering::Relaxed),
            max_hold_cycles: self.max_hold_cycles.load(Ordering::Relaxed),
            order_violations: self.order_violations.load(Ordering::Relaxed),
        }
    }
}
pub struct Guard {
    lock: &'static Lock,
    token: Token,
    _cpu_bound: PhantomData<*mut ()>,
}
impl Guard {
    pub fn acquire(class: u8) -> Self {
        let lock = named(class);
        Self {
            lock,
            token: lock.acquire(),
            _cpu_bound: PhantomData,
        }
    }
    pub(crate) fn try_acquire(class: u8) -> Option<Self> {
        let lock = named(class);
        lock.try_acquire().map(|token| Self {
            lock,
            token,
            _cpu_bound: PhantomData,
        })
    }
}
pub(crate) fn rtc_guard() -> Guard {
    Guard::acquire(11)
}
pub(crate) fn pci_config_guard() -> Guard {
    Guard::acquire(10)
}
// Named storage is resident for the kernel lifetime and initialized before
// entry. Prepared physical release plans retain the exact token across C
// calls, rather than keeping a Rust guard or mutable table borrow alive.
pub(super) fn named(class: u8) -> &'static Lock {
    unsafe { &*named_owner(class) }
}
/// Passive admission check for a native caller in another kernel domain.
/// It never lends the lock/token or acquires an additional owner.
pub(crate) fn held(class: u8) -> bool {
    named(class).held()
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.lock.release(self.token);
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains exact resident C lock storage, and exclusive output token.
pub unsafe extern "C" fn r4os_rust_owner_acquire(lock: *const Lock, output: *mut Token) {
    unsafe { output.write((&*lock).acquire()) };
}
#[unsafe(no_mangle)]
/// # Safety
/// lock is the same live instance; token retains its exact CPU acquisition.
pub unsafe extern "C" fn r4os_rust_owner_release(lock: *const Lock, token: *const Token) {
    unsafe { (&*lock).release(token.read()) };
}
#[unsafe(no_mangle)]
/// # Safety
/// lock retains its resident C layout for this call.
pub unsafe extern "C" fn r4os_rust_owner_held(lock: *const Lock) -> bool {
    unsafe { (&*lock).held() }
}
#[unsafe(no_mangle)]
/// # Safety
/// lock is resident; output owns one non-overlapping writable C Stats value.
pub unsafe extern "C" fn r4os_rust_owner_stats(lock: *const Lock, output: *mut Stats) {
    unsafe { output.write((&*lock).stats()) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_owner_fault_allowed() -> bool {
    fault_allowed()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_owner_prepare_stack() {
    prepare_stack();
}
const _: () = {
    assert!(core::mem::size_of::<Token>() == 16 && core::mem::offset_of!(Token, cpu_index) == 8);
    assert!(core::mem::size_of::<Lock>() == 712 && core::mem::offset_of!(Lock, depth) == 4);
    assert!(
        core::mem::offset_of!(Lock, acquired_tsc) == 136
            && core::mem::offset_of!(Lock, acquisitions) == 648
    );
    assert!(core::mem::size_of::<Stats>() == 64);
};

pub(crate) const CLASS_NAMES: [&[u8]; 12] = [
    b"program_state",
    b"storage",
    b"network",
    b"driver_work",
    b"heap",
    b"virtual_memory",
    b"page_tables",
    b"physical_memory",
    b"boot_log",
    b"serial_output",
    b"pci_config",
    b"rtc",
];
pub(crate) fn stats_for_class(class: u8) -> Stats {
    assert!((class as usize) < CLASS_NAMES.len());
    named(class).stats()
}

unsafe fn owner_runtime_depth() -> u32 {
    unsafe { crate::arch::percpu::r4os_rust_percpu_depth().read() }
}

unsafe fn named_owner(class: u8) -> *const Lock {
    &NAMED[class as usize] as *const Lock
}
