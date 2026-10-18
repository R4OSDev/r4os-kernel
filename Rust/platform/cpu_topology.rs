//! One immutable local CPUID image per exact boot descriptor. BSP owns the
//! bounded plan; each CPU publishes only its own slot before Online. Sealing
//! checks qualified peers before the AP release barrier. No allocation/wait.
use super::cpu_topology_decode::{Discovery, Geometry, discover};
use crate::arch::smp_policy::MAX_CPUS;
use core::{
    arch::x86_64::CpuidResult,
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
};
struct Slot {
    described: AtomicU8,
    apic: UnsafeCell<u32>,
    epoch: UnsafeCell<u64>,
    capturing: AtomicBool,
    ready: AtomicBool,
    data: UnsafeCell<MaybeUninit<Discovery>>,
}
impl Slot {
    const fn new() -> Self {
        Self {
            described: AtomicU8::new(0),
            apic: UnsafeCell::new(0),
            epoch: UnsafeCell::new(0),
            capturing: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            data: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
    fn descriptor(&self) -> Option<(u32, u64)> {
        (self.described.load(Ordering::Acquire) == 2)
            .then(|| unsafe { (self.apic.get().read(), self.epoch.get().read()) })
    }
    // Only BSP startup describes slots; release publication precedes readers.
    fn describe(&self, apic: u32, epoch: u64) -> bool {
        if let Some(old) = self.descriptor() {
            return old == (apic, epoch);
        }
        if self
            .described
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        unsafe {
            self.apic.get().write(apic);
            self.epoch.get().write(epoch);
        }
        self.described.store(2, Ordering::Release);
        true
    }
    fn read(&self) -> Option<Discovery> {
        self.ready
            .load(Ordering::Acquire)
            .then(|| unsafe { (*self.data.get()).assume_init_read() })
    }
}
// BSP is the sole descriptor writer. A CAS admits one local data writer per
// slot. Data and descriptors are immutable after their release publications;
// readers acquire readiness before copying, and no reference escapes a copy.
unsafe impl Sync for Slot {}
static SLOTS: [Slot; MAX_CPUS] = [const { Slot::new() }; MAX_CPUS];
static COUNT: AtomicU32 = AtomicU32::new(0);
static PLAN_READY: AtomicBool = AtomicBool::new(false);
static SEALED: AtomicBool = AtomicBool::new(false);
static QUALIFIED: AtomicU64 = AtomicU64::new(0);
static TRUSTED: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy)]
pub(crate) struct Snapshot {
    pub index: u32,
    pub apic_id: u32,
    pub start_epoch: u64,
    pub sealed: bool,
    pub qualified: bool,
    pub trusted: bool,
    pub data: Option<Discovery>,
}
pub(crate) fn count() -> u32 {
    COUNT.load(Ordering::Acquire)
}

/// BSP startup only, before notifying APs. Full IDs include failed/pending
/// configured slots. The BSP retains its independent boot-lifetime epoch1.
pub(crate) fn seed_plan(ids: &[u32], epoch: u64) -> bool {
    if ids.is_empty() || ids.len() > MAX_CPUS || epoch == 0 || SEALED.load(Ordering::Acquire) {
        return false;
    }
    for (i, &id) in ids.iter().enumerate() {
        if id == u32::MAX || ids[..i].contains(&id) {
            return false;
        }
        let expected = (id, if i == 0 { 1 } else { epoch });
        if let Some(old) = SLOTS[i].descriptor()
            && old != expected
        {
            return false;
        }
    }
    if SLOTS[0].descriptor() != Some((ids[0], 1)) {
        return false;
    }
    if PLAN_READY.load(Ordering::Acquire) {
        return count() as usize == ids.len();
    }
    for (i, &id) in ids.iter().enumerate().skip(1) {
        if !SLOTS[i].describe(id, epoch) {
            return false;
        }
    }
    COUNT.store(ids.len() as u32, Ordering::Release);
    PLAN_READY.store(true, Ordering::Release);
    true
}

/// Executed by the exact CPU during startup. The supplied bounded local CPUID
/// reader cannot allocate, block or call a module. Unknown geometry is valid
/// data with a functional flat fallback; a mismatched slot/epoch is refused.
pub(crate) fn capture(
    index: u32,
    apic: u32,
    epoch: u64,
    query: impl FnMut(u32, u32) -> CpuidResult,
) -> bool {
    let Some(slot) = SLOTS.get(index as usize) else {
        return false;
    };
    if apic == u32::MAX || epoch == 0 {
        return false;
    }
    if index == 0
        && slot.described.load(Ordering::Acquire) == 0
        && (epoch != 1 || !slot.describe(apic, epoch))
    {
        return false;
    }
    if slot.descriptor() != Some((apic, epoch)) {
        return false;
    }
    if slot.ready.load(Ordering::Acquire) {
        return true;
    }
    if SEALED.load(Ordering::Acquire)
        || slot
            .capturing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return false;
    }
    let data = discover(apic, query);
    unsafe {
        (*slot.data.get()).write(data);
    }
    slot.ready.store(true, Ordering::Release);
    if index == 0 {
        let _ = COUNT.compare_exchange(0, 1, Ordering::Release, Ordering::Relaxed);
    }
    true
}
fn overlaps(a: u32, b: u32, bits_a: u32, bits_b: u32) -> bool {
    a >> bits_a.max(bits_b) == b >> bits_a.max(bits_b)
}
fn conflict(a: u32, ga: Geometry, b: u32, gb: Geometry) -> bool {
    (ga.package_shift != gb.package_shift && overlaps(a, b, ga.package_shift, gb.package_shift))
        || (ga.smt_shift != gb.smt_shift && overlaps(a, b, ga.smt_shift, gb.smt_shift))
}
/// Sole BSP call before the AP release barrier. A late failed AP may finish
/// its immutable raw image, but cannot join this qualified/trusted generation.
pub(crate) fn seal(qualified: u64) {
    if SEALED.load(Ordering::Acquire) {
        return;
    }
    let mut trusted = 0;
    for (i, slot) in SLOTS.iter().enumerate().take(count() as usize) {
        if qualified & (1 << i) != 0 && slot.read().is_some_and(|d| d.geometry.is_ok()) {
            trusted |= 1 << i;
        }
    }
    let local_valid = trusted;
    for (i, slot) in SLOTS.iter().enumerate().take(count() as usize) {
        if local_valid & (1 << i) == 0 {
            continue;
        }
        let (a, _) = slot.descriptor().unwrap();
        let ga = slot.read().unwrap().geometry.unwrap();
        for (j, slot) in SLOTS.iter().enumerate().take(count() as usize).skip(i + 1) {
            if local_valid & (1 << j) == 0 {
                continue;
            }
            let (b, _) = slot.descriptor().unwrap();
            let gb = slot.read().unwrap().geometry.unwrap();
            if conflict(a, ga, b, gb) {
                trusted &= !(1 << i);
                trusted &= !(1 << j);
            }
        }
    }
    QUALIFIED.store(qualified, Ordering::Relaxed);
    TRUSTED.store(trusted, Ordering::Relaxed);
    SEALED.store(true, Ordering::Release);
}
pub(crate) fn snapshot(index: u32) -> Option<Snapshot> {
    if index >= count() {
        return None;
    }
    let slot = &SLOTS[index as usize];
    let (apic_id, start_epoch) = slot.descriptor()?;
    let sealed = SEALED.load(Ordering::Acquire);
    Some(Snapshot {
        index,
        apic_id,
        start_epoch,
        sealed,
        qualified: sealed && QUALIFIED.load(Ordering::Relaxed) & (1 << index) != 0,
        trusted: sealed && TRUSTED.load(Ordering::Relaxed) & (1 << index) != 0,
        data: slot.read(),
    })
}
