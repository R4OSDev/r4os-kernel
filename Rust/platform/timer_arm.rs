//! Private functional BSP arm receipts. Hardware owners issue nonrepeating
//! revisions; the timer retains only copied keys, never an AP pause token.
use core::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Stamp(u64);
pub(crate) struct Revision(AtomicU64);
impl Revision {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }
    /// The existing exclusive hardware owner serializes every mutation.
    /// Exhaustion disables receipts; it never recycles an earlier authority.
    pub fn advance_owned(&self) {
        let old = self.0.load(Ordering::Relaxed);
        if old != u64::MAX {
            self.0.store(old + 1, Ordering::Relaxed);
        }
    }
    pub fn stamp(&self) -> Option<Stamp> {
        match self.0.load(Ordering::Relaxed) {
            0 | u64::MAX => None,
            value => Some(Stamp(value)),
        }
    }
    pub fn matches(&self, stamp: Stamp) -> bool {
        self.stamp() == Some(stamp)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Context {
    pub backend: u32,
    pub cpu: u32,
    pub epoch: u64,
    pub clock: u32,
}
impl Context {
    pub fn valid(self) -> bool {
        matches!(self.backend, 1 | 2)
            && self.cpu == 0
            && self.epoch != 0
            && self.clock != 0
            && self.clock != u32::MAX
    }
}
#[derive(Clone, Copy)]
struct Entry {
    requested: u64,
    checkpoint: u64,
    context: Context,
    stamp: Stamp,
}
pub(crate) struct Cache(Option<Entry>);
impl Cache {
    pub const EMPTY: Self = Self(None);
    pub fn clear(&mut self) {
        self.0 = None;
    }
    /// This is only a logical match. The hardware owner must additionally
    /// validate this exact stamp and its future/disarmed physical state.
    pub fn candidate(&self, requested: u64, now: u64, context: Context) -> Option<Stamp> {
        let entry = self.0?;
        if !context.valid() || entry.context != context || entry.requested != requested {
            return None;
        }
        if requested == u64::MAX {
            (entry.checkpoint == u64::MAX).then_some(entry.stamp)
        } else {
            (requested > now && entry.checkpoint != u64::MAX && entry.checkpoint > now)
                .then_some(entry.stamp)
        }
    }
    /// The caller has just confirmed the owner's exact physical state and
    /// unchanged context after a successful program/disarm. No telemetry key.
    pub fn remember(&mut self, requested: u64, checkpoint: u64, context: Context, stamp: Stamp) {
        self.0 = context.valid().then_some(Entry {
            requested,
            checkpoint,
            context,
            stamp,
        });
    }
}

/// A raw comparator may legitimately be zero across a counter wrap. Only a
/// nonzero forward distance within the owner's bounded arm interval is valid.
pub(crate) fn counter_future(now: u64, target: u64, mask: u64, maximum: u64) -> bool {
    let distance = target.wrapping_sub(now) & mask;
    distance != 0 && distance <= maximum
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> Context {
        Context {
            backend: 2,
            cpu: 0,
            epoch: 17,
            clock: 9,
        }
    }
    fn stamp() -> Stamp {
        let revision = Revision::new();
        revision.advance_owned();
        revision.stamp().unwrap()
    }
    #[test]
    fn mutation_revokes_exact_receipt_and_never_uses_zero() {
        let revision = Revision::new();
        assert_eq!(revision.stamp(), None);
        revision.advance_owned();
        let first = revision.stamp().unwrap();
        assert!(revision.matches(first));
        revision.advance_owned();
        assert!(!revision.matches(first));
        assert_ne!(revision.stamp(), Some(first));
    }
    #[test]
    fn exhaustion_permanently_refuses_receipts_without_wrapping() {
        let revision = Revision(AtomicU64::new(u64::MAX - 2));
        let before = revision.stamp().unwrap();
        revision.advance_owned();
        let last = revision.stamp().unwrap();
        assert_ne!(before, last);
        revision.advance_owned();
        assert_eq!(revision.stamp(), None);
        revision.advance_owned();
        assert_eq!(revision.stamp(), None);
        assert!(!revision.matches(last));
    }
    #[test]
    fn finite_request_requires_same_context_and_live_checkpoint() {
        let mut cache = Cache::EMPTY;
        let stamp = stamp();
        cache.remember(1000, 100, context(), stamp);
        assert_eq!(cache.candidate(1000, 99, context()), Some(stamp));
        for now in [100, 101, 1000] {
            assert_eq!(cache.candidate(1000, now, context()), None);
        }
        for request in [999, 1001, u64::MAX] {
            assert_eq!(cache.candidate(request, 90, context()), None);
        }
    }
    #[test]
    fn empty_receipt_retains_only_exact_live_bsp_context() {
        let mut cache = Cache::EMPTY;
        let stamp = stamp();
        let context = context();
        cache.remember(u64::MAX, u64::MAX, context, stamp);
        assert_eq!(cache.candidate(u64::MAX, u64::MAX, context), Some(stamp));
        for changed in [
            Context {
                backend: 1,
                ..context
            },
            Context { cpu: 1, ..context },
            Context {
                epoch: 18,
                ..context
            },
            Context {
                clock: 10,
                ..context
            },
        ] {
            assert_eq!(cache.candidate(u64::MAX, 0, changed), None);
        }
        cache.clear();
        assert_eq!(cache.candidate(u64::MAX, 0, context), None);
    }
    #[test]
    fn unknown_epoch_or_clock_cannot_authorize_even_empty_receipt() {
        for context in [
            Context {
                epoch: 0,
                ..context()
            },
            Context {
                clock: 0,
                ..context()
            },
            Context {
                clock: u32::MAX,
                ..context()
            },
            Context {
                backend: 0,
                ..context()
            },
        ] {
            let mut cache = Cache::EMPTY;
            cache.remember(u64::MAX, u64::MAX, context, stamp());
            assert_eq!(cache.candidate(u64::MAX, 0, context), None);
        }
    }
    #[test]
    fn raw_counter_future_accepts_zero_target_wrap_but_rejects_consumed_and_past() {
        assert!(counter_future(
            0xffff_fff0,
            0,
            u64::from(u32::MAX),
            0x7fff_ffff
        ));
        assert!(!counter_future(0, 0, u64::from(u32::MAX), 0x7fff_ffff));
        assert!(!counter_future(1, 0, u64::from(u32::MAX), 0x7fff_ffff));
        assert!(!counter_future(
            0,
            0x8000_0000,
            u64::from(u32::MAX),
            0x7fff_ffff
        ));
        assert!(counter_future(
            u64::MAX - 15,
            0,
            u64::MAX,
            0x3fff_ffff_ffff_ffff
        ));
        assert!(!counter_future(1, 0, u64::MAX, 0x3fff_ffff_ffff_ffff));
    }
}
