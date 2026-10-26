//! Observational wait statistics; never queue state or a wake publication.
//! Slots retain their totals across CPU offline/online. Choosing a slot before
//! migration is harmless: only the aggregate, not CPU attribution, is exposed.
use core::sync::atomic::{AtomicU64, Ordering};

#[repr(usize)]
#[derive(Clone, Copy)]
pub(super) enum Metric {
    QueueWaits = 0,
    WakeOne = 1,
    WakeAll = 2,
    Timeouts = 3,
    Cancellations = 4,
    Drops = 5,
    TotalWaitTicks = 6,
    MaxWaitTicks = 7,
    LastWaitTicks = 8,
    DirectedWakes = 9,
    DirectedScans = 10,
    DirectedFifoBypasses = 11,
}
const COUNT: usize = 12;
#[repr(align(128))]
struct Slot([AtomicU64; COUNT]);
#[repr(align(128))]
struct Last(AtomicU64);
pub(super) struct Counters<const N: usize> {
    slots: [Slot; N],
    // "Last" means last published completion globally, not an arbitrary CPU's
    // last completion. Keep that one store separate from local RMW counters.
    last: Last,
}
impl<const N: usize> Counters<N> {
    pub const fn new() -> Self {
        assert!(N > 0);
        Self {
            slots: [const { Slot([const { AtomicU64::new(0) }; COUNT]) }; N],
            last: Last(AtomicU64::new(0)),
        }
    }
    fn slot(&self, cpu: usize) -> &Slot {
        &self.slots[if cpu < N { cpu } else { 0 }]
    }
    #[inline]
    pub fn add(&self, cpu: usize, metric: Metric, amount: u64) {
        debug_assert!(!matches!(
            metric,
            Metric::LastWaitTicks | Metric::MaxWaitTicks
        ));
        self.slot(cpu).0[metric as usize].fetch_add(amount, Ordering::Relaxed);
    }
    pub fn latency(&self, cpu: usize, elapsed: u64) {
        let slot = self.slot(cpu);
        slot.0[Metric::TotalWaitTicks as usize].fetch_add(elapsed, Ordering::Relaxed);
        self.last.0.store(elapsed, Ordering::Relaxed);
        slot.0[Metric::MaxWaitTicks as usize].fetch_max(elapsed, Ordering::Relaxed);
    }
    // As before, independent fields are not a transactional instant while
    // producers run. After they quiesce, sums/max are exact, including offline
    // slots. No reset, task scan, IPI or producer rendezvous is needed.
    pub fn snapshot(&self) -> [u64; COUNT] {
        let mut values = [0_u64; COUNT];
        for slot in &self.slots {
            for (index, total) in values.iter_mut().enumerate() {
                if index == Metric::LastWaitTicks as usize {
                    continue;
                }
                let value = slot.0[index].load(Ordering::Relaxed);
                *total = if index == Metric::MaxWaitTicks as usize {
                    (*total).max(value)
                } else {
                    total.wrapping_add(value)
                };
            }
        }
        values[Metric::LastWaitTicks as usize] = self.last.0.load(Ordering::Relaxed);
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_cpus_and_shared_slot_keep_all_events() {
        let counters = Counters::<4>::new();
        std::thread::scope(|scope| {
            for producer in 0..8 {
                let counters = &counters;
                scope.spawn(move || {
                    for _ in 0..2000 {
                        counters.add(producer % 4, Metric::QueueWaits, 1);
                        counters.latency(producer % 4, producer as u64 + 1);
                    }
                });
            }
        });
        counters.latency(2, 3); // Most recent publication is not the largest.
        let values = counters.snapshot();
        assert_eq!(values[Metric::QueueWaits as usize], 16000);
        assert_eq!(values[Metric::TotalWaitTicks as usize], 72003);
        assert_eq!(values[Metric::MaxWaitTicks as usize], 8);
        assert_eq!(values[Metric::LastWaitTicks as usize], 3);
    }
    #[test]
    fn fallback_migration_and_retained_offline_slot() {
        let counters = Counters::<4>::new();
        counters.add(usize::MAX, Metric::WakeOne, 2); // Bootstrap fallback.
        counters.add(3, Metric::WakeOne, 3); // Slot 3 may later be offline.
        counters.add(1, Metric::WakeOne, 5); // Producer resumed on another CPU.
        for metric in [
            Metric::WakeAll,
            Metric::Timeouts,
            Metric::Cancellations,
            Metric::Drops,
            Metric::DirectedWakes,
            Metric::DirectedScans,
            Metric::DirectedFifoBypasses,
        ] {
            counters.add(3, metric, 7);
            assert_eq!(counters.snapshot()[metric as usize], 7);
        }
        assert_eq!(counters.snapshot()[Metric::WakeOne as usize], 10);
        assert_eq!(counters.snapshot(), counters.snapshot()); // Read never resets.
    }
    #[test]
    fn totals_wrap_but_maximum_and_global_last_do_not_sum() {
        let counters = Counters::<2>::new();
        counters.add(0, Metric::QueueWaits, u64::MAX);
        counters.add(1, Metric::QueueWaits, 2);
        counters.latency(0, u64::MAX);
        counters.latency(1, 2);
        let values = counters.snapshot();
        assert_eq!(values[Metric::QueueWaits as usize], 1);
        assert_eq!(values[Metric::TotalWaitTicks as usize], 1);
        assert_eq!(values[Metric::MaxWaitTicks as usize], u64::MAX);
        assert_eq!(values[Metric::LastWaitTicks as usize], 2);
        assert_eq!(core::mem::align_of::<Slot>(), 128);
        assert_eq!(core::mem::size_of::<Slot>(), 128);
        assert_eq!(core::mem::align_of::<Last>(), 128);
    }
}
