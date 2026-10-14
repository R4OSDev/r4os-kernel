//! Ranked, CPU-bound no-sleep owners. Resident C-layout storage can be
//! declared by a not-yet-migrated owner, but only Rust mutates the lock state.
use super::{layout, owner_policy};
use crate::build_bridge::config;
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};
const MAX_CPUS: usize = 32;
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
static HELD_RANK: [AtomicU8; MAX_CPUS] = [const { AtomicU8::new(0) }; MAX_CPUS];
unsafe extern "C" {
    fn r4os_zig_owner_cpu() -> u32;
    fn r4os_zig_owner_runtime_depth() -> u32;
    fn r4os_zig_named_owner(class: u8) -> *const Lock;
}
fn cpu() -> usize {
    (unsafe { r4os_zig_owner_cpu() }) as usize
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
    HELD_RANK[cpu()].load(Ordering::Relaxed) == 0 && unsafe { r4os_zig_owner_runtime_depth() } == 0
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
            if config::ENABLE_METRICS {
                spins = spins.saturating_add(1);
            }
            // Existing IF=1 callers remain able to service TLB IPIs while
            // contending; IRQ/no-sleep IF=0 callers never borrow interrupts.
            if saved & IF != 0 {
                sti();
                pause();
                cli();
            } else {
                pause();
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
}
// Named storage is resident for the kernel lifetime and initialized before
// entry. Prepared physical release plans retain the exact token across C
// calls, rather than keeping a Rust guard or mutable table borrow alive.
pub(super) fn named(class: u8) -> &'static Lock {
    unsafe { &*r4os_zig_named_owner(class) }
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
    assert!(core::mem::size_of::<Lock>() == 392 && core::mem::offset_of!(Lock, depth) == 4);
    assert!(
        core::mem::offset_of!(Lock, acquired_tsc) == 72
            && core::mem::offset_of!(Lock, acquisitions) == 328
    );
    assert!(core::mem::size_of::<Stats>() == 64);
};
