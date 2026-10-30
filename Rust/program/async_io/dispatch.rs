//! Bounded lane admission, serialized by the Async owner. A notified receiver
//! reserves one queued opportunity until its exact worker resumes or retires.
use core::sync::atomic::{AtomicU32, Ordering};
pub(crate) const WIDTH: usize = 4;
pub(crate) const REQUESTS: usize = 64;
#[derive(Clone, Copy)]
pub(crate) struct Lane {
    queued: usize,
    waiting: u8,
    notified: u8,
    cursor: usize,
}
impl Lane {
    pub(crate) const EMPTY: Self = Self {
        queued: 0,
        waiting: 0,
        notified: 0,
        cursor: 0,
    };
    pub(crate) fn enqueue(&mut self) {
        assert!(self.queued < REQUESTS, "async queued admission overflow");
        self.queued += 1;
    }
    pub(crate) fn remove_request(&mut self) {
        assert!(self.queued != 0, "async queued admission underflow");
        self.queued -= 1;
    }
    fn bit(slot: usize) -> u8 {
        assert!(slot < WIDTH, "async receiver slot");
        1 << slot
    }
    /// Real resumption or exact-generation retirement consumes its wake credit.
    pub(crate) fn withdraw(&mut self, slot: usize) {
        let bit = Self::bit(slot);
        self.waiting &= !bit;
        self.notified &= !bit;
    }
    /// Empty predicate and enrollment are one Async-owner operation. The
    /// resident AUTO Event bridges the later unlock-to-physical-wait interval.
    pub(crate) fn park(&mut self, slot: usize) {
        let bit = Self::bit(slot);
        assert!(self.notified & bit == 0, "async credited receiver parking");
        self.waiting |= bit;
    }
    pub(crate) fn unreserved(&self) -> bool {
        self.queued > self.notified.count_ones() as usize
    }
    pub(crate) fn reserve(&mut self, active: u8) -> Option<usize> {
        if !self.unreserved() {
            return None;
        }
        let eligible = self.waiting & active;
        for offset in 0..WIDTH {
            let slot = (self.cursor + offset) % WIDTH;
            let bit = Self::bit(slot);
            if eligible & bit != 0 {
                self.waiting &= !bit;
                self.notified |= bit;
                self.cursor = (slot + 1) % WIDTH;
                return Some(slot);
            }
        }
        None
    }
}

/// Resident Service receiver hints, not Request/endpoint ownership. Endpoint
/// callbacks cannot acquire the Async mutex. A receiver enrolls BEFORE its
/// final epoch check; the resident AUTO Event bridges the later physical wait.
/// Both this enrollment and the bucket epoch use SeqCst: acquire/release alone
/// permits the callback and final check to miss each other's publication.
pub(crate) struct ReceiverHints {
    state: AtomicU32,
}
impl ReceiverHints {
    pub(crate) const fn new() -> Self {
        Self {
            state: AtomicU32::new(0),
        }
    }
    const MASK: u32 = (1 << WIDTH) - 1;
    const CURSOR: usize = 2 * WIDTH;
    fn bit(slot: usize) -> u32 {
        u32::from(Lane::bit(slot))
    }
    /// Consume this exact receiver's previous credit and publish its next
    /// final check. No Task or Request address is exposed to the notifier.
    pub(crate) fn enroll(&self, slot: usize) -> bool {
        let bit = Self::bit(slot);
        let old = self
            .state
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                Some((state & !(bit << WIDTH)) | bit)
            })
            .unwrap();
        old & (bit << WIDTH) != 0
    }
    pub(crate) fn withdraw(&self, slot: usize) -> bool {
        let bit = Self::bit(slot);
        self.state
            .fetch_and(!(bit | (bit << WIDTH)), Ordering::SeqCst)
            & (bit << WIDTH)
            != 0
    }
    /// Coalesced scout uses limit1. Only the Async owner can establish a larger
    /// demand from actual detached continuation candidates. Existing credits
    /// reserve retries until the receiver's next check or exact retirement.
    pub(crate) fn reserve(&self, limit: usize) -> Option<usize> {
        let limit = limit.min(WIDTH);
        let mut observed = self.state.load(Ordering::SeqCst);
        loop {
            if ((observed >> WIDTH) & Self::MASK).count_ones() as usize >= limit {
                return None;
            }
            let cursor = (observed >> Self::CURSOR) as usize % WIDTH;
            let slot = (0..WIDTH)
                .map(|offset| (cursor + offset) % WIDTH)
                .find(|&slot| observed & Self::bit(slot) != 0)?;
            let bit = Self::bit(slot);
            let next = ((observed & !bit) | (bit << WIDTH)) & !(3 << Self::CURSOR)
                | (((slot + 1) % WIDTH) as u32) << Self::CURSOR;
            match self.state.compare_exchange_weak(
                observed,
                next,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Some(slot),
                Err(current) => observed = current,
            }
        }
    }
}

#[cfg(test)]
mod receiver_tests {
    use super::*;
    fn parked() -> ReceiverHints {
        let hints = ReceiverHints::new();
        for slot in 0..WIDTH {
            assert!(!hints.enroll(slot));
        }
        hints
    }
    #[test]
    fn one_scout_coalesces_until_real_resumption() {
        let hints = parked();
        assert_eq!(hints.reserve(1), Some(0));
        for _ in 0..100 {
            assert_eq!(hints.reserve(1), None);
        }
        assert!(hints.enroll(0));
        assert!(!hints.withdraw(0));
        assert_eq!(hints.reserve(1), Some(1));
    }
    #[test]
    fn actual_demand_keeps_other_receivers_credits() {
        let hints = parked();
        for slot in 0..WIDTH {
            assert_eq!(hints.reserve(WIDTH), Some(slot));
        }
        assert_eq!(hints.reserve(99), None);
        assert!(hints.enroll(0));
        assert_eq!(hints.reserve(3), None);
        assert!(!hints.withdraw(0));
        assert!(hints.withdraw(1));
        assert!(!hints.enroll(1));
        assert_eq!(hints.reserve(3), Some(1));
    }
    #[test]
    fn exact_withdraw_preserves_unrelated_receiver() {
        let hints = parked();
        assert_eq!(hints.reserve(2), Some(0));
        assert_eq!(hints.reserve(2), Some(1));
        assert!(hints.withdraw(0));
        assert!(!hints.withdraw(0));
        assert!(hints.enroll(1));
        assert!(!hints.enroll(0));
        assert_eq!(hints.reserve(1), Some(2));
    }
    #[test]
    fn simultaneous_callbacks_reserve_one_scout() {
        let hints = parked();
        let start = std::sync::Barrier::new(WIDTH);
        let selected = std::thread::scope(|scope| {
            let mut joins = std::vec::Vec::new();
            for _ in 0..WIDTH {
                joins.push(scope.spawn(|| {
                    start.wait();
                    hints.reserve(1)
                }));
            }
            joins
                .into_iter()
                .map(|join| join.join().unwrap())
                .filter(Option::is_some)
                .count()
        });
        assert_eq!(selected, 1);
    }
    #[test]
    fn all_disjoint_states_match_independent_demand_and_rotation() {
        let mut cases = 0;
        for waiting in 0..16u32 {
            for credited in 0..16u32 {
                if waiting & credited != 0 {
                    continue;
                }
                for cursor in 0..WIDTH {
                    for limit in 0..=WIDTH {
                        cases += 1;
                        let before = waiting | (credited << WIDTH) | (cursor as u32) << 8;
                        let hints = ReceiverHints {
                            state: AtomicU32::new(before),
                        };
                        let expected = if credited.count_ones() as usize >= limit {
                            None
                        } else {
                            (0..WIDTH)
                                .map(|offset| (cursor + offset) % WIDTH)
                                .find(|&slot| waiting & (1 << slot) != 0)
                        };
                        assert_eq!(hints.reserve(limit), expected);
                        let after = hints.state.load(Ordering::SeqCst);
                        if let Some(slot) = expected {
                            assert_eq!(after & 15, waiting & !(1 << slot));
                            assert_eq!((after >> 4) & 15, credited | (1 << slot));
                            assert_eq!(after >> 8, ((slot + 1) % WIDTH) as u32);
                        } else {
                            assert_eq!(after, before);
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 1620);
    }
    #[test]
    fn delayed_event_does_not_recreate_retired_hint() {
        let hints = parked();
        let old = hints.reserve(1).unwrap();
        assert!(hints.withdraw(old));
        // A delayed resident Event signal can only cause an extra fresh check;
        // it never recreates this retired credit or supplies a Task identity.
        assert!(!hints.enroll(old));
        assert!(!hints.withdraw(old));
        assert_eq!(hints.reserve(1), Some(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn waiting() -> Lane {
        let mut lane = Lane::EMPTY;
        for i in 0..WIDTH {
            lane.park(i);
        }
        lane
    }
    #[test]
    fn one_request_one_receiver() {
        let mut lane = waiting();
        lane.enqueue();
        assert_eq!(lane.reserve(15), Some(0));
        assert_eq!(lane.reserve(15), None);
        assert_eq!((lane.waiting, lane.notified), (14, 1));
        lane.withdraw(0);
        assert!(lane.unreserved());
        lane.remove_request();
        assert!(!lane.unreserved());
        assert_eq!(lane.reserve(15), None);
    }
    #[test]
    fn burst_preserves_four_credits_against_busy_receiver() {
        let mut lane = waiting();
        for _ in 0..REQUESTS {
            lane.enqueue();
        }
        for i in 0..WIDTH {
            assert_eq!(lane.reserve(15), Some(i));
        }
        assert_eq!(lane.reserve(15), None);
        lane.withdraw(0);
        for _ in 0..REQUESTS - 3 {
            assert!(lane.unreserved());
            lane.remove_request();
        }
        assert!(!lane.unreserved());
        assert_eq!((lane.queued, lane.notified), (3, 14));
        for i in 1..WIDTH {
            lane.withdraw(i);
            assert!(lane.unreserved());
            lane.remove_request();
        }
        assert_eq!((lane.queued, lane.notified), (0, 0));
    }
    #[test]
    fn enrollment_orders_have_no_lost_work() {
        let mut early = Lane::EMPTY;
        early.enqueue();
        assert_eq!(early.reserve(1), None);
        assert!(early.unreserved()); // Recheck/claim before ever sleeping.
        early.remove_request();
        early.park(0);
        assert_eq!(early.reserve(1), None);
        let mut late = Lane::EMPTY;
        late.park(0); // Publish enrollment before releasing the owner.
        late.enqueue();
        assert_eq!(late.reserve(1), Some(0));
        late.withdraw(0); // Event can be signaled before physical enrollment.
        assert!(late.unreserved());
    }
    #[test]
    fn cancellation_does_not_manufacture_another_wake() {
        let mut lane = waiting();
        lane.enqueue();
        assert_eq!(lane.reserve(15), Some(0));
        lane.remove_request();
        assert_eq!(lane.reserve(15), None);
        lane.withdraw(0);
        assert!(!lane.unreserved());
        lane.park(0);
        assert_eq!(lane.reserve(15), None);
    }
    #[test]
    fn retired_receiver_returns_only_its_credit() {
        let mut lane = waiting();
        lane.enqueue();
        assert_eq!(lane.reserve(15), Some(0));
        lane.withdraw(0);
        assert_eq!(lane.reserve(14), Some(1));
        assert_eq!(lane.reserve(14), None);
        lane.withdraw(0); // Replaced generation cannot withdraw slot1's credit.
        assert_eq!(lane.notified, 2);
    }
    #[test]
    fn lane_isolation_and_rotation() {
        let mut file = waiting();
        let service = waiting();
        for i in 0..8 {
            file.enqueue();
            assert_eq!(file.reserve(15), Some(i % WIDTH));
            file.withdraw(i % WIDTH);
            file.remove_request();
            file.park(i % WIDTH);
        }
        assert_eq!(
            (service.queued, service.waiting, service.notified),
            (0, 15, 0)
        );
    }
    #[test]
    fn every_small_state_matches_independent_demand_bound() {
        for queued in 0..=REQUESTS {
            for waiting in 0u8..16 {
                for notified in 0u8..16 {
                    if waiting & notified != 0 {
                        continue;
                    }
                    for active in 0u8..16 {
                        let mut lane = Lane {
                            queued,
                            waiting,
                            notified,
                            cursor: 3,
                        };
                        let expected = queued
                            .saturating_sub(notified.count_ones() as usize)
                            .min((waiting & active).count_ones() as usize);
                        let mut selected = 0u8;
                        while let Some(slot) = lane.reserve(active) {
                            assert_eq!(selected & (1 << slot), 0);
                            selected |= 1 << slot;
                        }
                        assert_eq!(selected.count_ones() as usize, expected);
                        assert_eq!(selected & !(waiting & active), 0);
                        assert_eq!(lane.notified, notified | selected);
                    }
                }
            }
        }
    }
}
