//! Resident one-shot terminal FP records. Capture has no owner/heap/wait/callback.
//! Current task identity is copied from a CPU-local atomic publication, never
//! by borrowing mutable scheduler state in the fatal path.
use super::{fpu, percpu, smp_policy::MAX_CPUS};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
pub(crate) mod data {
    include!("fpu_fault_data.rs");
}
#[repr(C, align(64))]
struct Legacy([u8; 512]);
#[derive(Clone, Copy, Default)]
pub(crate) struct Snapshot {
    pub vector: u32,
    pub rip: u64,
    pub cpu: Option<(u32, u32)>,
    pub task: Option<(u32, u64)>,
    pub backend: u32,
    pub mask: u64,
    pub configured_mask: u64,
    pub mask_known: bool,
    pub bytes: u32,
    pub cr0: u64,
    pub cr4: u64,
    pub unavailable: u32,
    pub environment: Option<data::Environment>,
}
impl Snapshot {
    const EMPTY: Self = Self {
        vector: 0,
        rip: 0,
        cpu: None,
        task: None,
        backend: 0,
        mask: 0,
        configured_mask: 0,
        mask_known: false,
        bytes: 0,
        cr0: 0,
        cr4: 0,
        unavailable: 0,
        environment: None,
    };
}
struct Identity {
    seq: AtomicU64,
    id: AtomicU32,
    generation: AtomicU64,
}
impl Identity {
    const fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            id: AtomicU32::new(0),
            generation: AtomicU64::new(0),
        }
    }
}
static TASKS: [Identity; MAX_CPUS] = [const { Identity::new() }; MAX_CPUS];
pub(crate) fn publish_task(cpu: u32, id: u32, generation: u64) {
    let Some(s) = TASKS.get(cpu as usize) else {
        return;
    };
    let n = s.seq.load(Ordering::Relaxed);
    s.seq.store(n.wrapping_add(1), Ordering::Release);
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
    s.id.store(id, Ordering::Relaxed);
    s.generation.store(generation, Ordering::Relaxed);
    s.seq.store(n.wrapping_add(2), Ordering::Release);
}
fn task(cpu: u32) -> Option<(u32, u64)> {
    let s = TASKS.get(cpu as usize)?;
    let n = s.seq.load(Ordering::Acquire);
    if n & 1 != 0 {
        return None;
    }
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
    let id = s.id.load(Ordering::Relaxed);
    let generation = s.generation.load(Ordering::Relaxed);
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
    if id == 0 || generation == 0 || n != s.seq.load(Ordering::Acquire) {
        None
    } else {
        Some((id, generation))
    }
}
struct Slot {
    claimed: AtomicBool,
    ready: AtomicBool,
    raw: UnsafeCell<Legacy>,
    record: UnsafeCell<Snapshot>,
}
impl Slot {
    const fn new() -> Self {
        Self {
            claimed: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            raw: UnsafeCell::new(Legacy([0; 512])),
            record: UnsafeCell::new(Snapshot::EMPTY),
        }
    }
}
// Each fatal record is written once on its exact CPU (or shared early slot).
// The private diagnostic record is acquired nonblocking and released after copy.
unsafe impl Sync for Slot {}
static FATAL: [Slot; MAX_CPUS + 1] = [const { Slot::new() }; MAX_CPUS + 1];
static DIAGNOSTIC: Slot = Slot::new();
pub(crate) fn fatal_records() -> usize {
    FATAL
        .iter()
        .filter(|s| s.ready.load(Ordering::Acquire))
        .count()
}
pub(crate) fn retained(cpu: usize) -> Option<Snapshot> {
    let slot = FATAL.get(cpu)?;
    if !slot.ready.load(Ordering::Acquire) {
        return None;
    }
    Some(unsafe { slot.record.get().read_volatile() })
}
fn remember(slot: &Slot, s: Snapshot) -> Snapshot {
    unsafe { slot.record.get().write_volatile(s) };
    slot.ready.store(true, Ordering::Release);
    s
}
fn registers() -> (u64, u64) {
    let cr0;
    let cr4;
    unsafe {
        core::arch::asm!("mov {},cr0","mov {},cr4",out(reg)cr0,out(reg)cr4,options(nomem,nostack,preserves_flags));
    }
    (cr0, cr4)
}
fn sample(slot: &Slot, vector: u32, rip: u64, cpu: Option<(u32, u32)>) -> Snapshot {
    let (cr0, cr4) = registers();
    let mut out = Snapshot {
        vector,
        rip,
        cpu,
        task: cpu.and_then(|(i, _)| task(i)),
        cr0,
        cr4,
        ..Snapshot::default()
    };
    let Some(d) = fpu::descriptor() else {
        out.unavailable = 1;
        return remember(slot, out);
    };
    out.backend = d.contract.backend;
    out.configured_mask = d.contract.mask;
    out.bytes = d.contract.bytes;
    let Some((index, _)) = cpu else {
        out.unavailable = 2;
        return remember(slot, out);
    };
    if !fpu::local_ready(index) {
        out.unavailable = 3;
        return remember(slot, out);
    }
    if cr0 & ((1 << 2) | (1 << 3)) != 0 || cr4 & (1 << 9) == 0 {
        out.unavailable = 4;
        return remember(slot, out);
    }
    if d.contract.standard() {
        if cr4 & (1 << 18) == 0 {
            out.unavailable = 5;
        } else {
            let low: u32;
            let high: u32;
            // Local ready + architectural CR4.OSXSAVE qualify XGETBV(0).
            unsafe {
                core::arch::asm!("xgetbv",in("ecx")0u32,out("eax")low,out("edx")high,options(nomem,nostack,preserves_flags));
            }
            out.mask = u64::from(low) | (u64::from(high) << 32);
            out.mask_known = true;
            if out.mask != out.configured_mask {
                out.unavailable = 6;
            }
        }
    }
    // FXSAVE is nonwaiting. No FWAIT/FNINIT/EMMS/LDMXCSR/restore is issued.
    let raw = slot.raw.get();
    unsafe {
        core::arch::asm!("fxsave64 [{}]",in(reg)raw,options(nostack));
        out.environment = Some(data::decode(&(*raw).0));
    }
    remember(slot, out)
}
pub(crate) fn capture(vector: u32, rip: u64) -> Option<Snapshot> {
    if !matches!(vector, 16 | 19) {
        return None;
    }
    let cpu = percpu::checked_current();
    let index = cpu.map_or(MAX_CPUS, |(i, _)| i as usize);
    let slot = &FATAL[index];
    if slot
        .claimed
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return None;
    }
    Some(sample(slot, vector, rip, cpu))
}
/// Normal nonfaulting sampling uses separate storage and does not consume a
/// terminal record. It masks only local IRQs and owns no scheduler admission.
pub(crate) fn diagnostic() -> Option<Snapshot> {
    let flags = super::runtime::save_local();
    let out = if DIAGNOSTIC
        .claimed
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
    {
        let s = sample(&DIAGNOSTIC, 0, 0, percpu::checked_current());
        DIAGNOSTIC.claimed.store(false, Ordering::Release);
        Some(s)
    } else {
        None
    };
    super::runtime::restore_local(flags);
    out
}
pub(crate) fn emit(s: Snapshot) {
    let s = retained(s.cpu.map_or(MAX_CPUS, |(i, _)| i as usize)).unwrap_or(s);
    // The bounded resident record is already captured. Only existing bootlog/UART
    // outputs follow; no console, display lease or external module is called.
    fn text(b: &[u8]) {
        crate::boot::log::text(b);
        crate::boot::log_route::serial_write_raw(b);
    }
    fn number(mut n: u64) {
        let mut b = [0; 20];
        let mut i = b.len();
        loop {
            i -= 1;
            b[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        text(&b[i..]);
    }
    text(b"[FPFAULT47] vector=");
    number(s.vector.into());
    text(b" rip=");
    number(s.rip);
    text(b" cpu=");
    if let Some((i, apic)) = s.cpu {
        number(i.into());
        text(b" apic=");
        number(apic.into());
    } else {
        text(b"unknown");
    }
    text(b" native=");
    if let Some((id, generation)) = s.task {
        number(id.into());
        text(b" generation=");
        number(generation);
    } else {
        text(b"unknown");
    }
    text(b" backend=");
    number(s.backend.into());
    text(b" mask=");
    number(s.mask);
    text(b" mask_known=");
    number(s.mask_known as u64);
    text(b" configured_mask=");
    number(s.configured_mask);
    text(b" capture_status=");
    number(s.unavailable.into());
    text(b" active_bytes=");
    number(s.bytes.into());
    text(b" cr0=");
    number(s.cr0);
    text(b" cr4=");
    number(s.cr4);
    if let Some(e) = s.environment {
        text(b" cw=");
        number(e.cw.into());
        text(b" sw=");
        number(e.sw.into());
        text(b" ftw_abridged=");
        number(e.tags.into());
        text(b" top=");
        number(e.top.into());
        text(b" mxcsr=");
        number(e.mxcsr.into());
        text(b" mxcsr_mask=");
        number(e.mxcsr_mask.into());
        text(b" unmasked_x87=");
        number(e.unmasked_x87.into());
        text(b" unmasked_simd=");
        number(e.unmasked_simd.into());
    } else {
        text(b" unavailable=");
        number(s.unavailable.into());
    }
    text(b" terminal=1\r\n");
}
