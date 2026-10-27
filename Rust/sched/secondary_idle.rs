//! Local AP timer loans. Only the current CPU with IF=0 touches its token;
//! no reference/guard spans a stack switch, wait or foreign callback.
use crate::arch::{lapic, percpu, runtime};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
struct Slot(UnsafeCell<Option<lapic::PausedTimer>>);
unsafe impl Sync for Slot {}
static LOANS: [Slot; 64] = [const { Slot(UnsafeCell::new(None)) }; 64];

// Explicit diagnostic window only. No sampler and no receipt writes during
// ordinary idle/dispatch. Peers never read a non-atomic hardware token.
static OBSERVING: AtomicBool = AtomicBool::new(false);
struct Receipt {
    epoch: AtomicU64,
    entries: AtomicU64,
    returns: AtomicU64,
    retry_arms: AtomicU64,
    paused: AtomicBool,
    retrying: AtomicBool,
}
static RECEIPTS: [Receipt; 64] = [const {
    Receipt {
        epoch: AtomicU64::new(0),
        entries: AtomicU64::new(0),
        returns: AtomicU64::new(0),
        retry_arms: AtomicU64::new(0),
        paused: AtomicBool::new(false),
        retrying: AtomicBool::new(false),
    }
}; 64];
pub(crate) struct Observation;
impl Observation {
    pub(crate) fn begin() -> Option<Self> {
        let _runtime = runtime::RuntimeGuard::acquire();
        OBSERVING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        for r in &RECEIPTS {
            r.paused.store(false, Ordering::Release);
        }
        // Let already sleeping APs publish a real local receipt. This hint
        // neither creates work nor changes placement or schedulable capacity.
        for cpu in 1..64 {
            if percpu::is_schedulable(cpu) {
                crate::arch::smp::send_reschedule(cpu);
            }
        }
        Some(Self)
    }
}
impl Drop for Observation {
    fn drop(&mut self) {
        OBSERVING.store(false, Ordering::Release);
    }
}
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(crate) struct Sample {
    pub(crate) epoch: u64,
    pub(crate) entries: u64,
    pub(crate) returns: u64,
    pub(crate) retry_arms: u64,
    pub(crate) paused: bool,
    pub(crate) retrying: bool,
}
pub(crate) fn sample(cpu: u32) -> Option<Sample> {
    let r = RECEIPTS.get(cpu as usize)?;
    let returns = r.returns.load(Ordering::Acquire);
    let entries = r.entries.load(Ordering::Acquire);
    let out = Sample {
        epoch: r.epoch.load(Ordering::Acquire),
        entries,
        returns,
        retry_arms: r.retry_arms.load(Ordering::Acquire),
        paused: r.paused.load(Ordering::Acquire),
        retrying: r.retrying.load(Ordering::Acquire),
    };
    (returns == r.returns.load(Ordering::Acquire) && entries == r.entries.load(Ordering::Acquire))
        .then_some(out)
}
/// A reschedule edge also consumes an obsolete deadline loan even if no Task
/// becomes runnable; after demotion the next idle loan owns the count timer.
pub(crate) fn clock_changed() {
    let cpu = percpu::current_index() as usize;
    if cpu != 0
        && unsafe {
            (*LOANS[cpu].0.get())
                .as_ref()
                .is_some_and(|t| t.matches_current() && !t.clock_current())
        }
    {
        assert!(resume(), "idle timer source demotion");
    }
}
pub(crate) fn prepare(retry: Option<u64>) -> bool {
    let flags = runtime::save_local();
    let cpu = percpu::current_index() as usize;
    let observed = OBSERVING.load(Ordering::Relaxed);
    let result = if cpu == 0 {
        false
    } else {
        unsafe {
            let slot = &mut *LOANS[cpu].0.get();
            if slot.as_ref().is_some_and(|t| !t.matches_current()) {
                *slot = None;
            }
            if slot.is_none() {
                *slot = lapic::pause_secondary();
                if slot.is_some() && observed {
                    RECEIPTS[cpu].entries.fetch_add(1, Ordering::Release);
                }
            }
            let armed = slot
                .as_ref()
                .is_some_and(|t| lapic::arm_secondary_idle(t, retry));
            if observed {
                if armed && retry.is_some() {
                    RECEIPTS[cpu].retry_arms.fetch_add(1, Ordering::Release);
                }
                RECEIPTS[cpu].epoch.store(
                    crate::platform::cpu_topology::epoch(cpu as u32),
                    Ordering::Release,
                );
                RECEIPTS[cpu]
                    .retrying
                    .store(retry.is_some(), Ordering::Release);
                RECEIPTS[cpu].paused.store(armed, Ordering::Release);
            }
            if !armed && let Some(token) = slot.take() {
                assert!(
                    !token.matches_current() || lapic::finish_secondary_idle(token),
                    "idle arm failure must restore active timer"
                );
                if observed {
                    RECEIPTS[cpu].returns.fetch_add(1, Ordering::Release);
                }
            }
            armed
        }
    };
    runtime::restore_local(flags);
    result
}
pub(crate) fn resume() -> bool {
    let flags = runtime::save_local();
    let cpu = percpu::current_index() as usize;
    let result = if cpu == 0 {
        true
    } else {
        unsafe {
            let token = (&mut *LOANS[cpu].0.get()).take();
            if token.is_some() && OBSERVING.load(Ordering::Relaxed) {
                RECEIPTS[cpu].returns.fetch_add(1, Ordering::Release);
                RECEIPTS[cpu].paused.store(false, Ordering::Release);
            }
            token.is_none_or(|t| !t.matches_current() || lapic::finish_secondary_idle(t))
        }
    };
    runtime::restore_local(flags);
    result
}
