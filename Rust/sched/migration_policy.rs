//! Pure bounded exception policy. Native owners supply canonical role/budget,
//! topology and physical-idle facts; this module never admits a saved frame.
#[derive(Clone, Copy)]
pub(super) struct Budget {
    pub last_move: u64,
    pub last_exception: u64,
    pub latency_role: bool,
    pub boost_budget: bool,
    pub ticks: u32,
    pub dispatches: u16,
}
pub(super) fn cooling(last_move: u64, now: u64, cooldown: u64) -> bool {
    last_move != 0 && now.saturating_sub(last_move) < cooldown
}
impl Budget {
    pub fn available(self, now: u64, cooldown: u64) -> bool {
        now != 0
            && now >= self.last_move
            && cooling(self.last_move, now, cooldown)
            && self.latency_role
            && (!self.boost_budget || self.ticks != 0 && self.dispatches != 0)
            && (self.last_exception == 0
                || now >= self.last_exception && now - self.last_exception >= cooldown)
    }
}
pub(super) fn free_core(
    target: u32,
    allowed: u64,
    online: u64,
    core: u64,
    mut idle: impl FnMut(u32) -> bool,
) -> bool {
    if target >= 64
        || allowed & (1 << target) == 0
        || core & (1 << target) == 0
        || core & !online != 0
    {
        return false;
    }
    let mut members = core;
    while members != 0 {
        let cpu = members.trailing_zeros();
        members &= members - 1;
        if !idle(cpu) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> Budget {
        Budget {
            last_move: 100,
            last_exception: 0,
            latency_role: true,
            boost_budget: true,
            ticks: 8,
            dispatches: 4,
        }
    }
    #[test]
    fn alternating_busy_cores_cannot_replenish_the_exception_budget() {
        let mut b = budget();
        let mut exceptions = std::vec::Vec::new();
        for now in 101..=190 {
            // Ordinary migration remains possible after the full cooldown.
            if !cooling(b.last_move, now, 30) {
                b.last_move = now;
            }
            if b.available(now, 30) {
                exceptions.push(now);
                b.last_move = now;
                b.last_exception = now;
            }
        }
        assert_eq!(exceptions, [101, 131, 161]);
        assert!(exceptions.windows(2).all(|pair| pair[1] - pair[0] >= 30));
    }
    #[test]
    fn role_exhaustion_clock_order_and_zero_never_grant_an_exception() {
        let b = budget();
        assert!(
            !Budget {
                latency_role: false,
                ..b
            }
            .available(101, 30)
        );
        assert!(!Budget { ticks: 0, ..b }.available(101, 30));
        assert!(!Budget { dispatches: 0, ..b }.available(101, 30));
        assert!(!b.available(0, 30));
        assert!(!b.available(99, 30));
        assert!(!b.available(130, 30)); // Normal policy, not an exception.
        assert!(
            !Budget {
                last_exception: 102,
                ..b
            }
            .available(101, 30)
        );
        assert!(b.available(101, 30));
    }
    #[test]
    fn unmetered_interactive_role_still_has_a_bounded_migration_budget() {
        let mut b = Budget {
            boost_budget: false,
            ticks: 0,
            dispatches: 0,
            ..budget()
        };
        assert!(b.available(101, 30));
        b.last_move = 101;
        b.last_exception = 101;
        assert!((102..131).all(|now| !b.available(now, 30)));
        // A later ordinary move does not manufacture an earlier exception.
        b.last_move = 129;
        assert!(!b.available(130, 30));
        assert!(b.available(131, 30));
    }
    #[test]
    fn smt_peer_occupancy_and_unknown_or_partial_cores_are_not_free() {
        let allowed = 1 << 2;
        let online = 0b1111;
        assert!(free_core(2, allowed, online, 0b1100, |_| true));
        assert!(!free_core(2, allowed, online, 0b1100, |cpu| cpu != 3));
        assert!(!free_core(2, allowed, online, 0, |_| true));
        assert!(!free_core(2, allowed, 0b0111, 0b1100, |_| true));
        assert!(!free_core(1, allowed, online, 0b0010, |_| true));
        assert!(!free_core(64, u64::MAX, u64::MAX, u64::MAX, |_| true));
        // Idle siblings outside the task's hard CPU set still need checking.
        let mut seen = 0;
        assert!(free_core(2, allowed, online, 0b1100, |cpu| {
            seen |= 1 << cpu;
            true
        }));
        assert_eq!(seen, 0b1100);
    }
}
