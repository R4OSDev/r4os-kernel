//! Exact local synchronous PMU ownership. Discovery/result queries are passive;
//! hardware instructions occur only in an explicitly admitted pure work scope.
use super::{
    cpu_capabilities::{self as catalog, LocalIrq},
    cpu_capabilities_store::Store,
    cpu_pmu_controller::{self as engine, Hardware},
    cpu_pmu_decode as decode, cpu_topology, monotonic,
};
use crate::{
    arch::{cpu_lifecycle as life, percpu, safe_msr, smp_policy::MAX_CPUS},
    contract as c,
    sched::task_context::Admission,
};
use core::{
    marker::PhantomData,
    mem,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
const LOCKING: u64 = u64::MAX;
// Each admitted generation can publish at most once, sequence+=2. Refuse
// exhaustion before hardware rather than losing the final publication space.
const LAST_GENERATION: u64 = (u64::MAX / 2) - 1;
struct Slot {
    claim: AtomicU64,
    claim_epoch: AtomicU64,
    next: AtomicU64,
    quarantined: AtomicBool,
    // Store's monotonic publication key is a measurement generation here;
    // the actual CPU execution epoch is retained independently in the words.
    last: Store<32>,
}
impl Slot {
    const fn new() -> Self {
        Self {
            claim: AtomicU64::new(0),
            claim_epoch: AtomicU64::new(0),
            next: AtomicU64::new(1),
            quarantined: AtomicBool::new(false),
            last: Store::new(),
        }
    }
    fn acquire(&self, epoch: u64) -> Result<Claim<'_>, i32> {
        if self.quarantined.load(Ordering::Acquire) {
            return Err(c::cpu_capabilities_error_busy);
        }
        self.claim
            .compare_exchange(0, LOCKING, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| c::cpu_capabilities_error_busy)?;
        let generation = match self
            .next
            .try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                (old != 0 && old <= LAST_GENERATION).then(|| old + 1)
            }) {
            Ok(value) => value,
            Err(_) => {
                let _ =
                    self.claim
                        .compare_exchange(LOCKING, 0, Ordering::AcqRel, Ordering::Acquire);
                return Err(c::cpu_capabilities_error_busy);
            }
        };
        self.claim_epoch.store(epoch, Ordering::Release);
        self.claim.store(generation, Ordering::Release);
        Ok(Claim {
            slot: self,
            generation,
            retained: false,
            _task_bound: PhantomData,
        })
    }
    fn snapshot(&self) -> Option<c::CpuPmuResult> {
        let retained = self.last.snapshot()?;
        let mut value: c::CpuPmuResult = unsafe { mem::transmute(retained.words) };
        if value.generation != retained.epoch {
            return None;
        }
        value.sequence = retained.sequence;
        Some(value)
    }
}
struct Claim<'a> {
    slot: &'a Slot,
    generation: u64,
    retained: bool,
    _task_bound: PhantomData<*mut ()>,
}
impl Claim<'_> {
    fn retain(&mut self) {
        self.retained = true;
        self.slot.quarantined.store(true, Ordering::Release);
    }
}
impl Drop for Claim<'_> {
    fn drop(&mut self) {
        if !self.retained {
            let _ = self.slot.claim.compare_exchange(
                self.generation,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }
}
static SLOTS: [Slot; MAX_CPUS] = [const { Slot::new() }; MAX_CPUS];
pub(crate) fn info(index: u32) -> Result<c::CpuPmuInfo, i32> {
    if index >= cpu_topology::count() {
        return Err(c::cpu_capabilities_error_index);
    }
    let epoch = cpu_topology::epoch(index);
    let snapshot = catalog::at_epoch(index, epoch)?;
    let d = decode::discover(&snapshot.raw);
    let slot = &SLOTS[index as usize];
    let claim = slot.claim.load(Ordering::Acquire);
    let quarantined = slot.quarantined.load(Ordering::Acquire);
    let online = percpu::is_online(index) && percpu::is_schedulable(index);
    let mut out = c::CpuPmuInfo {
        cpu_index: index,
        epoch,
        profile: d.profile,
        reason: d.reason,
        pmu_version: d.version,
        counter_count: d.counters,
        counter_width: d.width,
        fixed_count: d.fixed_counters,
        fixed_width: d.fixed_width,
        event_mask: d.events,
        architectural_event_mask: d.architectural_events,
        signature: d.signature,
        configured_cpus: cpu_topology::count(),
        ..Default::default()
    };
    if d.version != 0 {
        out.flags |= c::cpu_pmu_flag_exposed;
    }
    if d.virtual_cpu {
        out.flags |= c::cpu_pmu_flag_virtual;
    }
    if d.reason == decode::NONE {
        out.flags |= c::cpu_pmu_flag_qualified;
    }
    if d.profile == decode::MILAN_B1 {
        out.flags |= c::cpu_pmu_flag_modulo_only;
    }
    if online {
        out.flags |= c::cpu_pmu_flag_online;
    } else {
        out.reason = c::cpu_pmu_reason_offline;
    }
    if claim != 0 {
        out.flags |= c::cpu_pmu_flag_busy;
        out.state = c::cpu_pmu_state_active;
        if claim != LOCKING {
            out.inflight_generation = claim;
            out.inflight_epoch = slot.claim_epoch.load(Ordering::Acquire);
        }
    }
    if quarantined {
        out.flags |= c::cpu_pmu_flag_quarantined;
        out.state = c::cpu_pmu_state_quarantined;
        out.reason = c::cpu_pmu_reason_quarantined;
    }
    if let Some(last) = slot.snapshot() {
        out.flags |= c::cpu_pmu_flag_last_result;
        out.last_generation = last.generation;
        out.last_epoch = last.epoch;
        out.last_sequence = last.sequence;
    }
    Ok(out)
}
pub(crate) fn result(index: u32, epoch: u64, generation: u64) -> Result<c::CpuPmuResult, i32> {
    if index >= cpu_topology::count() {
        return Err(c::cpu_capabilities_error_index);
    }
    let value = SLOTS[index as usize]
        .snapshot()
        .ok_or(c::cpu_capabilities_error_busy)?;
    if value.epoch != epoch || value.generation != generation || epoch == 0 || generation == 0 {
        return Err(c::cpu_capabilities_error_epoch);
    }
    Ok(value)
}
struct Native {
    index: u32,
    epoch: u64,
    clock_generation: u32,
    callback: c::PmuWorkFn,
    context: u64,
}
impl Hardware for Native {
    fn current(&self) -> bool {
        catalog::flags() & (1 << 9) == 0
            && self.index == percpu::current_index()
            && cpu_topology::epoch(self.index) == self.epoch
            && percpu::is_online(self.index)
            && percpu::is_schedulable(self.index)
            && !life::is_terminal()
    }
    fn now(&self) -> Option<u64> {
        if !self.current() {
            return None;
        }
        let stamp = monotonic::capture();
        if !matches!(stamp.kind, 1 | 2) || stamp.generation != self.clock_generation {
            return None;
        }
        monotonic::resolve(stamp)
    }
    fn read(&self, msr: u32) -> Result<u64, engine::Error> {
        unsafe { safe_msr::read_exact(self.index, self.epoch, msr) }.map_err(|e| match e {
            safe_msr::Error::Busy => engine::Error::Busy,
            safe_msr::Error::Fault => engine::Error::Fault,
        })
    }
    fn write(&self, msr: u32, value: u64) -> engine::Write {
        match unsafe { safe_msr::write_exact(self.index, self.epoch, msr, value) } {
            Ok(()) => engine::Write::Written,
            Err(safe_msr::Error::Busy) => engine::Write::NotSubmitted,
            Err(safe_msr::Error::Fault) => engine::Write::Faulted,
        }
    }
    fn work(&self, iterations: u32) -> i32 {
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        // SAFETY: the trusted caller supplied a copied resident pure callback
        // with this explicit bounded nonblocking contract. IF0 retains exact
        // physical CPU, Admission its task/module, ProbeLease its membership.
        let value = unsafe { (self.callback)(self.context, iterations) };
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        value
    }
}
/// The public output copy also retains exact task admission and, unless
/// quarantined, lifecycle membership. No runtime owner spans that lifetime.
pub(crate) struct Measurement {
    pub value: c::CpuPmuResult,
    // Fields drop in declaration order: the exact task must remain admitted
    // until after its lifecycle/resource receipt has really been returned.
    _probe: Option<life::ProbeLease>,
    _admission: Admission,
}
pub(crate) fn measure(
    index: u32,
    epoch: u64,
    event: u32,
    iterations: u32,
    callback: c::PmuWorkFn,
    context: u64,
) -> Result<Measurement, i32> {
    if index >= cpu_topology::count() {
        return Err(c::cpu_capabilities_error_index);
    }
    if epoch == 0 || cpu_topology::epoch(index) != epoch {
        return Err(c::cpu_capabilities_error_epoch);
    }
    if event > decode::CACHE_MISSES || iterations == 0 || iterations > c::cpu_pmu_max_iterations {
        return Err(c::cpu_capabilities_error_invalid);
    }
    if catalog::flags() & (1 << 9) == 0
        || !crate::memory::owner::fault_allowed()
        || percpu::current_index() != index
    {
        return Err(c::cpu_capabilities_error_context);
    }
    let snapshot = catalog::at_epoch(index, epoch)?;
    let discovery = decode::discover(&snapshot.raw);
    let plan = decode::plan(&discovery, event).ok_or(c::cpu_capabilities_error_unsupported)?;
    let admission = Admission::acquire(true).ok_or(c::cpu_capabilities_error_context)?;
    let probe = life::probe_lease().ok_or(c::cpu_capabilities_error_busy)?;
    crate::memory::owner::prepare_stack();
    let clock = monotonic::capture();
    if !matches!(clock.kind, 1 | 2) {
        return Err(c::cpu_capabilities_error_unsupported);
    }
    let original_if = catalog::flags() & (1 << 9);
    if original_if == 0 {
        return Err(c::cpu_capabilities_error_context);
    }
    let irq = LocalIrq::acquire();
    let native = Native {
        index,
        epoch,
        clock_generation: clock.generation,
        callback,
        context,
    };
    if !native.current() {
        return Err(c::cpu_capabilities_error_context);
    }
    let slot = &SLOTS[index as usize];
    let mut claim = slot.acquire(epoch)?;
    let outcome = engine::run(
        plan,
        event,
        index,
        epoch,
        claim.generation,
        iterations,
        &native,
    );
    let mut value = outcome.value;
    value.original_if = original_if;
    value.signature = discovery.signature;
    value.configured_cpus = cpu_topology::count();
    let mut probe = Some(probe);
    if !outcome.release {
        claim.retain();
        // Uncertain hardware is retained in resident atomic metadata and
        // cannot be bypassed by logical offline/restart or another session.
        mem::forget(probe.take());
    }
    drop(irq);
    value.final_if = catalog::flags() & (1 << 9);
    if value.final_if == value.original_if {
        value.flags |= c::cpu_pmu_result_flag_if_preserved;
    } else {
        value.flags &=
            !(c::cpu_pmu_result_flag_absolute_count | c::cpu_pmu_result_flag_modulo_delta);
        value.status = c::cpu_pmu_status_identity_changed;
    }
    let words: [u64; 32] = unsafe { mem::transmute(value) };
    let sequence = match slot.last.publish(claim.generation, words) {
        Ok(sequence) => sequence,
        Err(_) => {
            claim.retain();
            mem::forget(probe.take());
            return Err(c::cpu_capabilities_error_busy);
        }
    };
    value.sequence = sequence;
    // Only pure atomic metadata remains after IRQ restoration, so its exact
    // original slot can safely be released even if the caller then migrated.
    drop(claim);
    Ok(Measurement {
        value,
        _probe: probe,
        _admission: admission,
    })
}
const _: () = {
    assert!(mem::size_of::<c::CpuPmuInfo>() == 128);
    assert!(mem::size_of::<c::CpuPmuResult>() == 256);
    assert!(decode::INTEL == c::cpu_pmu_profile_intel_arch);
    assert!(decode::MILAN_B1 == c::cpu_pmu_profile_amd_milan_b1);
    assert!(decode::CYCLES == c::cpu_pmu_event_cycles);
    assert!(decode::INSTRUCTIONS == c::cpu_pmu_event_instructions);
    assert!(decode::BRANCHES == c::cpu_pmu_event_branches);
    assert!(decode::BRANCH_MISSES == c::cpu_pmu_event_branch_misses);
    assert!(decode::CACHE_REFERENCES == c::cpu_pmu_event_cache_references);
    assert!(decode::CACHE_MISSES == c::cpu_pmu_event_cache_misses);
    assert!(decode::NONE == c::cpu_pmu_reason_none);
    assert!(decode::NO_PMU == c::cpu_pmu_reason_no_pmu);
    assert!(decode::VIRTUAL == c::cpu_pmu_reason_virtual_cpu);
    assert!(decode::LAYOUT == c::cpu_pmu_reason_unsupported_layout);
    assert!(decode::MODEL == c::cpu_pmu_reason_unsupported_model);
    assert!(decode::MALFORMED == c::cpu_pmu_reason_malformed);
    assert!(decode::NO_MSR == c::cpu_pmu_reason_no_msr);
};
