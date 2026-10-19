//! Exact request/ACK generation and bounded cross-CPU invalidation owner.
//! The page-table owner serializes requests. IPI completion stays independent
//! of runtime admission; no Rust state reference crosses a sender or wait.
use super::{percpu, smp_policy::MAX_CPUS};
use crate::memory::owner;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
pub const VECTOR: u8 = 0xf2;
const PAGE: u64 = 4096;
const FULL_RELOAD: u64 = 64;
const NORMAL_LIMIT: u64 = 25_000_000;
const DIAGNOSTIC_LIMIT: u64 = 250_000;
type Sender = unsafe extern "C" fn(u32, u8) -> bool;
static SENDER: AtomicUsize = AtomicUsize::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static REQUEST_GENERATION: AtomicU64 = AtomicU64::new(0);
static REQUEST_BASE: AtomicU64 = AtomicU64::new(0);
static REQUEST_PAGES: AtomicU64 = AtomicU64::new(0);
static REQUEST_TARGET: AtomicU64 = AtomicU64::new(0);
static ACKS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static DROP_ACK: AtomicU64 = AtomicU64::new(0);
static DIAGNOSTIC: AtomicBool = AtomicBool::new(false);
static REQUESTS: AtomicU64 = AtomicU64::new(0);
static SUCCESSES: AtomicU64 = AtomicU64::new(0);
static TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static EXPECTED_TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static IPIS_SENT: AtomicU64 = AtomicU64::new(0);
static ACKNOWLEDGEMENTS: AtomicU64 = AtomicU64::new(0);
static OFFLINE_EXCLUSIONS: AtomicU64 = AtomicU64::new(0);
static LOCAL_PAGES: AtomicU64 = AtomicU64::new(0);
static LOCAL_RELOADS: AtomicU64 = AtomicU64::new(0);
static MAX_WAIT: AtomicU64 = AtomicU64::new(0);
unsafe extern "C" {
    fn r4os_read_cr3() -> u64;
    fn r4os_write_cr3(value: u64);
    fn r4os_invlpg(address: usize);
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Result {
    pub ok: bool,
    pub generation: u64,
    pub target_mask: u64,
    pub acknowledgement_mask: u64,
    pub offline_mask: u64,
    pub spins: u64,
}
#[repr(C)]
pub struct Stats {
    pub requests: u64,
    pub successes: u64,
    pub timeouts: u64,
    pub expected_timeouts: u64,
    pub ipis_sent: u64,
    pub acknowledgements: u64,
    pub offline_exclusions: u64,
    pub local_page_flushes: u64,
    pub local_full_reloads: u64,
    pub max_wait_spins: u64,
    pub generation: u64,
}
fn online_mask() -> u64 {
    percpu::tlb_mask()
}
/// Includes global translations. Only the current quiescent CPU uses this
/// barrier; it touches resident kernel storage and retains its idle stack.
pub(crate) fn flush_all_local() {
    let cr4: u64;
    unsafe {
        core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        if cr4 & (1 << 7) != 0 {
            core::arch::asm!("mov cr4, {}", in(reg) cr4 & !(1 << 7), options(nostack, preserves_flags));
            core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nostack, preserves_flags));
        } else {
            r4os_write_cr3(r4os_read_cr3());
        }
    }
    LOCAL_RELOADS.fetch_add(1, Ordering::Relaxed);
}
/// The page-table owner excludes a new request while the exact parked epoch
/// leaves participation. No old request or ACK is promoted to a new epoch.
pub(crate) fn withdraw_cpu(parked: &super::cpu_lifecycle::Parked) -> Option<u64> {
    let index = parked.index();
    if !owner::held(6)
        || DIAGNOSTIC.load(Ordering::Acquire)
        || crate::platform::cpu_topology::epoch(index) != parked.epoch()
    {
        return None;
    }
    let generation = REQUEST_GENERATION.load(Ordering::Acquire);
    if REQUEST_TARGET.load(Ordering::Acquire) & (1u64 << index) != 0
        && ACKS[index as usize].load(Ordering::Acquire) != generation
    {
        return None;
    }
    percpu::withdraw_tlb(parked).then_some(generation)
}
pub(crate) fn rearm_cpu(stopped: &super::cpu_lifecycle::Stopped) -> bool {
    let index = stopped.index();
    if !stopped.current()
        || !owner::held(6)
        || percpu::state(index) != percpu::OFFLINE
        || percpu::tlb_mask() & (1u64 << index) != 0
    {
        return false;
    }
    ACKS[index as usize].store(0, Ordering::Release);
    true
}
fn flush_local(base: u64, pages: u64) {
    if pages >= FULL_RELOAD {
        unsafe { r4os_write_cr3(r4os_read_cr3()) };
        LOCAL_RELOADS.fetch_add(1, Ordering::Relaxed);
        return;
    }
    for page in 0..pages {
        unsafe { r4os_invlpg((base + page * PAGE) as usize) };
    }
    LOCAL_PAGES.fetch_add(pages, Ordering::Relaxed);
}
fn acknowledgement_mask(target: u64, generation: u64) -> u64 {
    let mut mask = 0;
    for (index, ack) in ACKS.iter().enumerate() {
        let bit = 1u64 << index;
        if target & bit != 0 && ack.load(Ordering::Acquire) == generation {
            mask |= bit;
        }
    }
    mask
}
fn invalidate(base: u64, pages: u64) -> Result {
    if pages == 0 || base & (PAGE - 1) != 0 || !owner::held(6) {
        return Result::default();
    }
    REQUESTS.fetch_add(1, Ordering::Relaxed);
    flush_local(base, pages);
    let current = percpu::current_index();
    let target = online_mask() & !(1 << current);
    let mut generation = GENERATION.load(Ordering::Relaxed).wrapping_add(1);
    if generation == 0 {
        generation = 1;
    }
    GENERATION.store(generation, Ordering::Relaxed);
    REQUEST_BASE.store(base, Ordering::Release);
    REQUEST_PAGES.store(pages, Ordering::Release);
    REQUEST_TARGET.store(target, Ordering::Release);
    REQUEST_GENERATION.store(generation, Ordering::Release);
    let mut result = Result {
        generation,
        target_mask: target,
        ..Result::default()
    };
    if target == 0 {
        SUCCESSES.fetch_add(1, Ordering::Relaxed);
        result.ok = true;
        return result;
    }
    let sender = SENDER.load(Ordering::Acquire);
    if sender == 0 {
        TIMEOUTS.fetch_add(1, Ordering::Relaxed);
        return result;
    }
    // SAFETY: singleton boot registers this exact static C callback. No
    // reference to mutable request state lives across foreign AP notification.
    let sender: Sender = unsafe { core::mem::transmute(sender) };
    for index in 0..MAX_CPUS as u32 {
        if target & (1 << index) == 0 {
            continue;
        }
        if unsafe { sender(index, VECTOR) } {
            IPIS_SENT.fetch_add(1, Ordering::Relaxed);
        } else if percpu::state(index) != 4 {
            OFFLINE_EXCLUSIONS.fetch_add(1, Ordering::Relaxed);
        }
    }
    let diagnostic = DIAGNOSTIC.load(Ordering::Acquire);
    let limit = if diagnostic {
        DIAGNOSTIC_LIMIT
    } else {
        NORMAL_LIMIT
    };
    let mut observed_offline = 0;
    for spins in 0..limit {
        let acknowledged = acknowledgement_mask(target, generation);
        let offline = target & !online_mask();
        let new_offline = offline & !observed_offline;
        if new_offline != 0 {
            OFFLINE_EXCLUSIONS.fetch_add(u64::from(new_offline.count_ones()), Ordering::Relaxed);
            observed_offline |= new_offline;
        }
        if (acknowledged | offline) & target == target {
            SUCCESSES.fetch_add(1, Ordering::Relaxed);
            MAX_WAIT.fetch_max(spins, Ordering::Relaxed);
            result.ok = true;
            result.acknowledgement_mask = acknowledged | offline;
            result.offline_mask = offline;
            result.spins = spins;
            return result;
        }
        core::hint::spin_loop();
    }
    let acknowledged = acknowledgement_mask(target, generation);
    let offline = target & !online_mask();
    TIMEOUTS.fetch_add(1, Ordering::Relaxed);
    if diagnostic {
        EXPECTED_TIMEOUTS.fetch_add(1, Ordering::Relaxed);
    }
    MAX_WAIT.fetch_max(limit, Ordering::Relaxed);
    result.acknowledgement_mask = acknowledged | offline;
    result.offline_mask = offline;
    result.spins = limit;
    result
}
pub(crate) fn registered() -> bool {
    SENDER.load(Ordering::Acquire) != 0
}
pub(crate) fn isolated_injection_active() -> bool {
    DIAGNOSTIC.load(Ordering::Acquire)
}
/// A fault may contend for a memory owner with IF=0 while that owner waits
/// for this CPU's invalidation. Service only the lock-free TLB request; keep
/// interrupt flags and the caller's existing owners intact. The isolated
/// DROP_ACK probe retains its original IRQ-only missing-ACK contract.
pub(crate) fn service_pending() {
    // Capture before the diagnostic flag: observing an injected generation
    // also observes its earlier DIAGNOSTIC publication. Never reread a newer
    // generation after that check or consume DROP_ACK through this poll.
    let generation = REQUEST_GENERATION.load(Ordering::Acquire);
    if DIAGNOSTIC.load(Ordering::Acquire) {
        return;
    }
    if generation != 0
        && ACKS[percpu::current_index() as usize].load(Ordering::Acquire) != generation
    {
        handle_request(generation, false);
    }
}
pub fn handle_ipi() {
    let generation = REQUEST_GENERATION.load(Ordering::Acquire);
    handle_request(generation, true);
}
fn handle_request(generation: u64, allow_drop: bool) {
    if generation == 0 {
        return;
    }
    let index = percpu::current_index();
    let bit = 1u64 << index;
    if REQUEST_TARGET.load(Ordering::Acquire) & bit == 0 {
        return;
    }
    let base = REQUEST_BASE.load(Ordering::Acquire);
    let pages = REQUEST_PAGES.load(Ordering::Acquire);
    flush_local(base, pages);
    if allow_drop && DROP_ACK.load(Ordering::Acquire) & bit != 0 {
        DROP_ACK.fetch_and(!bit, Ordering::AcqRel);
        return;
    }
    ACKS[index as usize].store(generation, Ordering::Release);
    ACKNOWLEDGEMENTS.fetch_add(1, Ordering::Relaxed);
}
/// # Safety
/// Singleton boot supplies a valid static C callback or null. No native Zig
/// calling convention may be cast into this registration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tlb_register(sender: Option<Sender>) {
    SENDER.store(
        sender.map_or(0, |sender| sender as usize),
        Ordering::Release,
    );
}
/// # Safety
/// Caller supplies exclusive aligned Result output and retains the page-table
/// owner. Failure leaves mapped-frame retention with the existing caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tlb_invalidate(base: u64, pages: u64, output: *mut Result) {
    unsafe { output.write(invalidate(base, pages)) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tlb_control(operation: u8, index: u32) -> bool {
    match operation {
        0 => {
            handle_ipi();
            true
        }
        1 => {
            if index >= MAX_CPUS as u32
                || index == percpu::current_index()
                || percpu::state(index) != 4
            {
                return false;
            }
            DROP_ACK.store(1 << index, Ordering::Release);
            DIAGNOSTIC.store(true, Ordering::Release);
            true
        }
        2 => {
            DIAGNOSTIC.store(false, Ordering::Release);
            DROP_ACK.store(0, Ordering::Release);
            true
        }
        _ => panic!("Invalid private TLB operation"),
    }
}
/// # Safety
/// Caller supplies exclusive aligned Stats output; loads remain passive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tlb_stats(output: *mut Stats) {
    unsafe {
        output.write(Stats {
            requests: REQUESTS.load(Ordering::Relaxed),
            successes: SUCCESSES.load(Ordering::Relaxed),
            timeouts: TIMEOUTS.load(Ordering::Relaxed),
            expected_timeouts: EXPECTED_TIMEOUTS.load(Ordering::Relaxed),
            ipis_sent: IPIS_SENT.load(Ordering::Relaxed),
            acknowledgements: ACKNOWLEDGEMENTS.load(Ordering::Relaxed),
            offline_exclusions: OFFLINE_EXCLUSIONS.load(Ordering::Relaxed),
            local_page_flushes: LOCAL_PAGES.load(Ordering::Relaxed),
            local_full_reloads: LOCAL_RELOADS.load(Ordering::Relaxed),
            max_wait_spins: MAX_WAIT.load(Ordering::Relaxed),
            generation: REQUEST_GENERATION.load(Ordering::Acquire),
        })
    };
}
const _: () = {
    assert!(core::mem::size_of::<Result>() == 48);
    assert!(core::mem::offset_of!(Result, generation) == 8);
    assert!(core::mem::size_of::<Stats>() == 88);
};
