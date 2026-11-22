//! Union of device-dispatched I/O intervals. The device metadata owner
//! serializes these transitions. An exact request ID rejects a stale completion.
#[derive(Clone, Copy)]
pub(crate) struct Instant {
    pub epoch: u32,
    pub ns: u64,
}

pub(crate) struct Activity {
    ids: [u64; 16],
    pending: u8,
    clock: u32,
    since: u64,
    total: u64,
    epoch: u64,
}
impl Activity {
    pub const EMPTY: Self = Self {
        ids: [0; 16],
        pending: 0,
        clock: 0,
        since: 0,
        total: 0,
        epoch: 1,
    };

    pub fn reset(&mut self) {
        self.clock = 0;
        self.total = 0;
        self.epoch = self.epoch.wrapping_add(1).max(1);
    }

    fn sync(&mut self, now: Option<Instant>) -> Option<u64> {
        let Some(t) = now else {
            if self.clock != 0 {
                self.reset();
            }
            return None;
        };
        if self.clock != t.epoch || t.ns < self.since {
            self.reset();
            self.clock = t.epoch;
            self.since = t.ns;
        }
        Some(t.ns)
    }

    pub fn begin(&mut self, slot: usize, id: u64, clock: impl FnOnce() -> Option<Instant>) {
        if self.ids[slot] != 0 || id == 0 {
            return;
        }
        if self.pending == 0 {
            if let Some(t) = self.sync(clock()) {
                self.since = t;
            }
        }
        self.ids[slot] = id;
        self.pending += 1;
    }
    pub fn end(&mut self, slot: usize, id: u64, clock: impl FnOnce() -> Option<Instant>) {
        if self.ids[slot] != id || id == 0 {
            return;
        }
        self.ids[slot] = 0;
        self.pending -= 1;
        if self.pending == 0 {
            if let Some(t) = self.sync(clock()) {
                self.total = self.total.saturating_add(t - self.since);
            }
        }
    }
    pub fn sample(&mut self, now: Option<Instant>) -> Option<(u64, u64)> {
        let t = self.sync(now)?;
        let open = if self.pending != 0 { t - self.since } else { 0 };
        Some((self.total.checked_add(open)?, self.epoch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn t(ns: u64) -> Option<Instant> {
        Some(Instant { epoch: 1, ns })
    }
    #[test]
    fn overlaps_and_stale_receipts_count_each_nanosecond_once() {
        let mut a = Activity::EMPTY;
        let epoch = a.sample(t(0)).unwrap().1;
        a.begin(0, 1, || t(20));
        a.begin(1, 2, || panic!("parallel start reads the clock"));
        assert_eq!(a.sample(t(45)), Some((25, epoch)));
        a.end(0, 1, || panic!("non-final completion reads the clock"));
        a.end(0, 1, || panic!("duplicate completion reads the clock"));
        a.end(1, 2, || t(90));
        assert_eq!(a.sample(t(110)), Some((70, epoch)));
        a.begin(1, 3, || t(130));
        a.end(1, 2, || panic!("stale completion reads the clock"));
        a.end(1, 3, || t(145));
        assert_eq!(a.sample(t(200)), Some((85, epoch)));
    }
    #[test]
    fn reset_and_unavailable_clock_rebase_open_intervals() {
        let mut a = Activity::EMPTY;
        a.begin(0, 8, || t(10));
        let old = a.sample(t(50)).unwrap();
        assert_eq!(old.0, 40);
        a.reset();
        assert!(a.sample(None).is_none());
        let new = a.sample(t(200)).unwrap();
        assert_ne!(new.1, old.1);
        assert_eq!(new.0, 0);
        assert_eq!(a.sample(t(225)), Some((25, new.1)));
        assert_eq!(a.sample(Some(Instant { epoch: 2, ns: 250 })).unwrap().0, 0);
    }
}
