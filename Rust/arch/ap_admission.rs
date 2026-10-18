//! Exact single-start AP admission. BSP arms one immutable identity/epoch;
//! that AP publishes rejection or qualification before lifecycle/IRQ release.
use super::smp_policy::MAX_CPUS;
use crate::build_config::config;
use core::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};

struct Record {
    epoch: AtomicU64,
    expected_id: AtomicU32,
    observed_id: AtomicU32,
    phase: AtomicU8,
    failure: AtomicU8,
}
impl Record {
    const fn empty() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            expected_id: AtomicU32::new(0),
            observed_id: AtomicU32::new(u32::MAX),
            phase: AtomicU8::new(0),
            failure: AtomicU8::new(0),
        }
    }
}
static RECORDS: [Record; MAX_CPUS] = [const { Record::empty() }; MAX_CPUS];
const REASONS: [&[u8]; 14] = [
    b"ap-identity",
    b"ap-epoch",
    b"xsave-components",
    b"x87-state",
    b"sse-state",
    b"ymm-state",
    b"xsave-layout",
    b"xsave-backend",
    b"osxsave",
    b"xcr0",
    b"xsave-size",
    b"fpu-controls",
    b"lapic-capabilities",
    b"tsc-capabilities",
];
pub(crate) fn prepare(index: u32, apic_id: u32, epoch: u64) -> bool {
    let Some(record) = RECORDS.get(index as usize) else {
        return false;
    };
    // Current lifecycle never reuses a failed/retired CPU or trampoline.
    // A future hotplug owner must introduce its own exact withdrawal first.
    if index == 0 || epoch == 0 || record.phase.load(Ordering::Acquire) != 0 {
        return false;
    }
    record.expected_id.store(apic_id, Ordering::Relaxed);
    record.epoch.store(epoch, Ordering::Relaxed);
    record.phase.store(1, Ordering::Release);
    true
}
pub(crate) fn begin(index: u32, apic_id: u32, epoch: u64) -> Result<(), &'static [u8]> {
    let Some(record) = RECORDS.get(index as usize) else {
        return Err(b"ap-identity");
    };
    if record.phase.load(Ordering::Acquire) != 1 || record.epoch.load(Ordering::Relaxed) != epoch {
        return Err(b"ap-epoch");
    }
    let observed = crate::platform::cpu::local_apic_id();
    record.observed_id.store(observed, Ordering::Relaxed);
    if record.expected_id.load(Ordering::Relaxed) != apic_id || observed != apic_id {
        return Err(b"ap-identity");
    }
    Ok(())
}
pub(crate) fn reject(index: u32, epoch: u64, reason: &'static [u8]) {
    let Some(record) = RECORDS.get(index as usize) else {
        return;
    };
    if record.phase.load(Ordering::Acquire) != 1 || record.epoch.load(Ordering::Relaxed) != epoch {
        return;
    }
    let code = if let Some(i) = REASONS.iter().position(|name| *name == reason) {
        (i + 1) as u8
    } else if let Some(i) = config::MODULE_CPU_REQUIREMENTS
        .iter()
        .position(|r| r.0 == reason)
    {
        (i + 32) as u8
    } else {
        u8::MAX
    };
    record.failure.store(code, Ordering::Relaxed);
    record.phase.store(3, Ordering::Release);
}
pub(crate) fn failure(index: u32, epoch: u64) -> Option<&'static [u8]> {
    let record = RECORDS.get(index as usize)?;
    if record.phase.load(Ordering::Acquire) != 3 || record.epoch.load(Ordering::Relaxed) != epoch {
        return None;
    }
    let code = record.failure.load(Ordering::Relaxed);
    Some(if (1..=REASONS.len() as u8).contains(&code) {
        REASONS[usize::from(code - 1)]
    } else if code >= 32 && usize::from(code - 32) < config::MODULE_CPU_REQUIREMENTS.len() {
        config::MODULE_CPU_REQUIREMENTS[usize::from(code - 32)].0
    } else {
        b"ap-qualification"
    })
}
pub(crate) fn qualify(index: u32, epoch: u64) -> bool {
    let Some(record) = RECORDS.get(index as usize) else {
        return false;
    };
    record.epoch.load(Ordering::Acquire) == epoch
        && record
            .phase
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
}
pub(crate) fn observed(index: u32, epoch: u64) -> Option<u32> {
    let record = RECORDS.get(index as usize)?;
    if record.phase.load(Ordering::Acquire) == 0 || record.epoch.load(Ordering::Relaxed) != epoch {
        return None;
    }
    let id = record.observed_id.load(Ordering::Relaxed);
    (id != u32::MAX).then_some(id)
}
pub(crate) fn qualified(index: u32, apic_id: u32, epoch: u64) -> bool {
    RECORDS.get(index as usize).is_some_and(|record| {
        record.phase.load(Ordering::Acquire) == 2
            && record.epoch.load(Ordering::Relaxed) == epoch
            && record.expected_id.load(Ordering::Relaxed) == apic_id
            && record.observed_id.load(Ordering::Relaxed) == apic_id
    })
}
