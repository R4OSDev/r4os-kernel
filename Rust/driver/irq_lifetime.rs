//! Exact IRQ registration lifetime, mutated only under runtime table admission.
//! A retired registration is unavailable to new dispatches but retains its
//! callback/context until its admitted call has returned and removal succeeds.
#[derive(Clone, Copy)]
pub(crate) struct Lifetime {
    pub generation: u64,
    pub active: bool,
    pub in_flight: bool,
    pub retiring: bool,
}
impl Lifetime {
    pub const EMPTY: Self = Self {
        generation: 0,
        active: false,
        in_flight: false,
        retiring: false,
    };
    pub const fn new(generation: u64) -> Self {
        Self {
            generation,
            active: true,
            ..Self::EMPTY
        }
    }
    pub const fn occupied(self) -> bool {
        self.active || self.retiring || self.in_flight
    }
    pub fn admit(&mut self, generation: u64) -> bool {
        if generation == 0 || self.generation != generation || !self.active || self.retiring {
            return false;
        }
        // The separate IRQ-vector owner serializes this registration's calls.
        assert!(!self.in_flight, "IRQ registration admitted concurrently");
        self.in_flight = true;
        true
    }
    pub fn retire(&mut self) {
        self.active = false;
        self.retiring = true;
    }
    pub fn finish(&mut self, generation: u64) {
        assert!(
            self.generation == generation && self.in_flight,
            "IRQ completion lost its retained registration"
        );
        self.in_flight = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retirement_blocks_admission_but_retains_the_exact_flight() {
        let mut a = Lifetime::new(7);
        assert!(a.admit(7));
        a.retire();
        assert!(a.occupied() && a.in_flight && a.retiring);
        assert!(!a.admit(7));
        a.finish(7);
        assert!(a.occupied() && !a.in_flight && !a.active);
    }
    #[test]
    fn replaced_generation_is_not_selected_by_an_old_irq_snapshot() {
        let old = Lifetime::new(17);
        let mut new = Lifetime::new(18);
        assert!(!new.admit(old.generation));
        assert!(new.admit(18));
        new.finish(18);
    }
    #[test]
    fn independent_registration_is_unaffected_by_a_retired_flight() {
        let mut a = Lifetime::new(3);
        let mut b = Lifetime::new(4);
        assert!(a.admit(3));
        a.retire();
        assert!(b.admit(4));
        b.finish(4);
        assert!(a.in_flight && !b.in_flight);
        a.finish(3);
    }
    #[test]
    #[should_panic(expected = "IRQ completion lost its retained registration")]
    fn wrong_generation_cannot_complete_a_flight() {
        let mut a = Lifetime::new(5);
        assert!(a.admit(5));
        a.finish(6);
    }
    #[test]
    #[should_panic(expected = "IRQ registration admitted concurrently")]
    fn parallel_calls_need_the_exact_irq_vector_owner() {
        let mut a = Lifetime::new(5);
        assert!(a.admit(5));
        a.admit(5);
    }
}
