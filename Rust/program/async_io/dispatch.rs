//! Bounded lane admission, serialized by the Async owner. A notified receiver
//! reserves one queued opportunity until its exact worker resumes or retires.
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
