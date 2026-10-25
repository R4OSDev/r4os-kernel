//! One acknowledged, still unconsumed doorbell for one validated CPU identity.
//! Scheduling pending, IRQ delivery and the CPU's execution epoch are distinct.
#[derive(Clone, Copy)]
pub(crate) struct Gate<K: Copy + Eq> {
    identity: Option<K>,
    in_flight: bool,
}
impl<K: Copy + Eq> Gate<K> {
    pub(crate) const EMPTY: Self = Self {
        identity: None,
        in_flight: false,
    };
    /// The caller has validated this current identity under its transport owner.
    pub(crate) fn request(&mut self, identity: K) -> bool {
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.in_flight = false;
        }
        if self.in_flight {
            return false;
        }
        self.in_flight = true;
        true
    }
    /// Only an actual vector receipt on that exact current CPU may consume it.
    pub(crate) fn received(&mut self, identity: K) -> bool {
        if self.identity != Some(identity) || !self.in_flight {
            return false;
        }
        self.in_flight = false;
        true
    }
    pub(crate) fn failed(&mut self, identity: K) {
        if self.identity == Some(identity) {
            self.in_flight = false;
        }
    }
    pub(crate) fn snapshot(&self) -> (Option<K>, bool) {
        (self.identity, self.in_flight)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const A: (u32, u32, u64) = (3, 0x1234, 9);
    const B: (u32, u32, u64) = (3, 0x1234, 10);
    #[test]
    fn burst_is_one_send_until_actual_receipt() {
        let mut gate = Gate::EMPTY;
        assert!(gate.request(A));
        for _ in 0..128 {
            assert!(!gate.request(A));
        }
        assert!(gate.received(A));
        assert!(gate.request(A));
    }
    #[test]
    fn request_after_consumption_needs_another_doorbell() {
        let mut gate = Gate::EMPTY;
        assert!(gate.request(A));
        assert!(gate.received(A));
        // A scheduler may still defer its pending decision. That state is
        // intentionally absent from this transport gate.
        assert!(gate.request(A));
        assert!(gate.received(A));
        assert!(gate.request(A)); // A subsequent idle entry needs a new wake.
    }
    #[test]
    fn new_epoch_cannot_inherit_old_in_flight_credit() {
        let mut gate = Gate::EMPTY;
        assert!(gate.request(A));
        assert!(gate.request(B));
        assert!(!gate.received(A));
        gate.failed(A);
        assert_eq!(gate.snapshot(), (Some(B), true));
        assert!(!gate.request(B));
        assert!(gate.received(B));
    }
    #[test]
    fn failed_delivery_allows_retry_without_forging_receipt() {
        let mut gate = Gate::EMPTY;
        assert!(gate.request(A));
        gate.failed(A);
        assert!(!gate.received(A));
        assert!(gate.request(A));
    }
    #[test]
    fn full_identity_and_independent_cpu_slots() {
        let mut first = Gate::EMPTY;
        let mut second = Gate::EMPTY;
        assert!(first.request(A));
        assert!(second.request(A));
        let other_apic = (A.0, 0x11234, A.2);
        assert!(first.request(other_apic));
        assert!(!first.received(A));
        assert!(!second.request(A));
        assert!(second.received(A));
    }
}
