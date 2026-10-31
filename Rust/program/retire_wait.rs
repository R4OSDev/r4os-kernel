//! Retry scheduling metadata under the existing Program/Async owner. A ready
//! hint permits another exact physical cleanup attempt, never completion/free.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wait {
    Progress,
    Retry,
    Blocked,
}
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct Retry {
    round: u64,
    deadline: u64,
    mode: u8,
    padding: [u8; 7],
}
impl Retry {
    pub(crate) const EMPTY: Self = Self {
        round: 0,
        deadline: 0,
        mode: 0,
        padding: [0; 7],
    };
    pub(crate) fn ready(&self, round: u64, now: u64) -> bool {
        self.mode != 3
            && (self.round == 0 || self.round != round || self.mode == 2 && now >= self.deadline)
    }
    pub(crate) fn begin(&mut self, round: u64) {
        assert_ne!(round, 0);
        self.round = round;
        self.mode = 0;
    }
    /// Preserve the begin marker: a round rollover can invalidate an exact
    /// in-progress record under its owner while the leaf runs unlocked.
    pub(crate) fn defer(&mut self, now: u64, wait: Wait) {
        self.mode = match wait {
            Wait::Progress => 1,
            Wait::Retry => 2,
            Wait::Blocked => 3,
        };
        self.deadline = if wait == Wait::Retry {
            now.saturating_add(1)
        } else {
            0
        };
    }
    pub(crate) fn deadline(&self) -> Option<u64> {
        (self.mode == 2).then_some(self.deadline)
    }
    pub(crate) fn blocked(&self) -> bool {
        self.mode == 3
    }
    pub(crate) fn reset_round(&mut self) {
        self.round = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_progress_wait_has_no_timer_retry() {
        let mut retry = Retry::EMPTY;
        retry.begin(7);
        retry.defer(100, Wait::Progress);
        for now in [100, 101, 1000, u64::MAX] {
            assert!(!retry.ready(7, now));
            assert_eq!(retry.deadline(), None);
        }
        assert!(retry.ready(8, 100));
    }
    #[test]
    fn an_explicit_unknown_retry_keeps_its_one_tick_boundary() {
        let mut retry = Retry::EMPTY;
        retry.begin(7);
        retry.defer(100, Wait::Retry);
        assert_eq!(retry.deadline(), Some(101));
        assert!(!retry.ready(7, 100));
        assert!(retry.ready(7, 101));
        assert!(retry.ready(7, 102));
        assert!(retry.ready(8, 100));
    }
    #[test]
    fn one_retry_deadline_does_not_make_another_progress_wait_due() {
        let mut event = Retry::EMPTY;
        let mut timed = Retry::EMPTY;
        event.begin(1);
        timed.begin(1);
        event.defer(20, Wait::Progress);
        timed.defer(20, Wait::Retry);
        assert!(timed.ready(1, 21));
        assert!(!event.ready(1, 21));
        assert!(event.ready(2, 21));
    }
    #[test]
    fn round_reset_covers_an_unlocked_in_progress_attempt() {
        let mut retry = Retry::EMPTY;
        retry.begin(u64::MAX);
        retry.reset_round();
        retry.defer(50, Wait::Progress);
        assert!(retry.ready(1, 50));
        retry.begin(1);
        retry.defer(50, Wait::Progress);
        assert!(!retry.ready(1, 50));
    }
    #[test]
    fn permanent_block_is_not_repaired_by_time_or_a_foreign_signal() {
        let mut retry = Retry::EMPTY;
        retry.begin(1);
        retry.defer(0, Wait::Blocked);
        retry.reset_round();
        assert!(retry.blocked());
        assert!(!retry.ready(2, u64::MAX));
        assert_eq!(retry.deadline(), None);
    }
    #[test]
    fn finite_max_deadline_is_distinct_from_an_event_wait() {
        let mut timed = Retry::EMPTY;
        timed.begin(9);
        timed.defer(u64::MAX - 1, Wait::Retry);
        assert_eq!(timed.deadline(), Some(u64::MAX));
        assert!(!timed.ready(9, u64::MAX - 1));
        assert!(timed.ready(9, u64::MAX));
        let mut event = Retry::EMPTY;
        event.begin(9);
        event.defer(u64::MAX - 1, Wait::Progress);
        assert!(!event.ready(9, u64::MAX));
    }
}
