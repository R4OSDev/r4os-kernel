//! CPU-local time spent in scheduler-admitted HLT and MWAIT. A reader and
//! a lifecycle transition need no interrupt, lock or callback on the target.
use super::{percpu, smp_policy::MAX_CPUS};
use crate::{
    contract as c,
    platform::{cpu_topology, monotonic},
};
use core::sync::atomic::{
    AtomicU64,
    Ordering::{Acquire, Relaxed, Release},
    fence,
};

#[repr(align(64))]
#[derive(Default)]
struct Counters {
    // Only the lifecycle owner writes these. Odd epochs are schedulable.
    epoch: AtomicU64,
    online_since: AtomicU64,
    // Only the local CPU, with interrupts disabled, writes the remaining fields.
    sequence: AtomicU64,
    idle_epoch: AtomicU64,
    since: AtomicU64, // nanoseconds + 1, or zero when executing
    completed: AtomicU64,
    wakes: AtomicU64,
}
static TIMES: [Counters; MAX_CPUS] = [const {
    Counters {
        epoch: AtomicU64::new(0),
        online_since: AtomicU64::new(0),
        sequence: AtomicU64::new(0),
        idle_epoch: AtomicU64::new(0),
        since: AtomicU64::new(0),
        completed: AtomicU64::new(0),
        wakes: AtomicU64::new(0),
    }
}; MAX_CPUS];

fn time() -> Option<(u32, u64)> {
    let stamp = monotonic::capture();
    Some((stamp.generation, monotonic::resolve(stamp)?))
}

/// The runtime owner serializes schedulability. An unpark starts a fresh
/// accounting interval even when the processor retains its execution epoch.
pub(crate) fn availability(cpu: u32, online: bool) {
    let t = &TIMES[cpu as usize];
    let epoch = t.epoch.load(Relaxed);
    if online == (epoch & 1 != 0) {
        return;
    }
    if online {
        let Some((_, now)) = time() else {
            return;
        };
        t.online_since.store(now, Relaxed);
    }
    t.epoch.store(epoch.wrapping_add(1), Release);
}
impl Counters {
    fn changing(&self) -> u64 {
        let s = self.sequence.load(Relaxed);
        self.sequence.store(s.wrapping_add(1), Relaxed);
        fence(Release);
        s.wrapping_add(2)
    }
    fn select(&self, epoch: u64) {
        if self.idle_epoch.load(Relaxed) != epoch {
            self.completed.store(0, Relaxed);
            self.wakes.store(0, Relaxed);
            self.idle_epoch.store(epoch, Relaxed);
        }
    }
}
pub(crate) fn enter(cpu: u32) {
    let t = &TIMES[cpu as usize];
    let epoch = t.epoch.load(Acquire);
    if epoch & 1 == 0 {
        return;
    }
    let Some((_, now)) = time() else {
        return;
    };
    let next = t.changing();
    t.select(epoch);
    t.since.store(now.saturating_add(1), Relaxed);
    t.sequence.store(next, Release);
}
/// Called first in IRQ/IPI dispatch. Interrupt work, including an immediate
/// context switch, therefore never extends a previously open idle interval.
#[inline]
pub(crate) fn leave(cpu: u32) {
    let t = &TIMES[cpu as usize];
    let since = t.since.load(Relaxed);
    if since == 0 {
        return;
    }
    let epoch = t.epoch.load(Acquire);
    let now = time();
    let next = t.changing();
    t.since.store(0, Relaxed);
    if let Some((_, end)) = now
        && epoch & 1 != 0
    {
        t.select(epoch);
        let start = (since - 1).max(t.online_since.load(Relaxed));
        t.completed.store(
            t.completed
                .load(Relaxed)
                .saturating_add(end.saturating_sub(start)),
            Relaxed,
        );
        t.wakes
            .store(t.wakes.load(Relaxed).saturating_add(1), Relaxed);
    }
    t.sequence.store(next, Release);
}
pub(crate) fn info(cpu: u32) -> Result<c::CpuTimeInfo, i32> {
    let count = cpu_topology::count();
    if cpu >= count {
        return Err(c::cpu_capabilities_error_invalid);
    }
    let t = &TIMES[cpu as usize];
    let mut result = c::CpuTimeInfo {
        cpu_index: cpu,
        configured_cpus: count,
        ..Default::default()
    };
    for _ in 0..8 {
        let epoch = t.epoch.load(Acquire);
        result.epoch = epoch;
        result.flags = u32::from(percpu::is_schedulable(cpu));
        if result.flags == 0 || epoch & 1 == 0 {
            return Ok(result);
        }
        let seq = t.sequence.load(Acquire);
        if seq & 1 != 0 {
            continue;
        }
        let origin = t.online_since.load(Relaxed);
        let same = t.idle_epoch.load(Relaxed) == epoch;
        let elapsed = if same { t.completed.load(Relaxed) } else { 0 };
        let wakes = if same { t.wakes.load(Relaxed) } else { 0 };
        let since = t.since.load(Relaxed);
        let Some((generation, end)) = time() else {
            return Ok(result);
        };
        fence(Acquire);
        if t.sequence.load(Relaxed) != seq || t.epoch.load(Acquire) != epoch {
            continue;
        }
        let open = if since == 0 {
            0
        } else {
            end.saturating_sub((since - 1).max(origin))
        };
        result.clock_generation = generation;
        result.online_ns = end.saturating_sub(origin);
        result.idle_ns = elapsed.saturating_add(open).min(result.online_ns);
        result.wake_count = wakes;
        result.flags |= 2;
        return Ok(result);
    }
    Err(c::cpu_capabilities_error_busy)
}
