//! One data request to either qualified channel transfers a 64-byte cache line.
pub(super) const COUNTER_MASK: u64 = (1 << 48) - 1;

pub(super) fn interval_bytes(previous: [u64; 2], current: [u64; 2], ns: u64) -> Option<u64> {
    if ns == 0 || ns >= 100_000_000_000 {
        return None;
    }
    let requests = (current[0].wrapping_sub(previous[0]) & COUNTER_MASK)
        + (current[1].wrapping_sub(previous[1]) & COUNTER_MASK);
    let bytes = requests * 64;
    // 128 GiB/s is above this dual-channel DDR4 profile. Allow one page of
    // skew between the two register reads. A reset starts a new epoch.
    let plausible = u128::from(bytes) * 1_000_000_000
        <= u128::from(ns) * (128u128 << 30) + 4096u128 * 1_000_000_000;
    plausible.then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_channels_wrap_without_a_spike() {
        assert_eq!(
            interval_bytes([COUNTER_MASK - 1, COUNTER_MASK], [1, 3], 1_000_000),
            Some(7 * 64)
        );
    }
    #[test]
    fn reset_and_stale_interval_do_not_fabricate_a_rate() {
        assert_eq!(interval_bytes([1000, 50], [0, 55], 1_000_000_000), None);
        assert_eq!(interval_bytes([0; 2], [0; 2], 0), None);
        assert_eq!(interval_bytes([0; 2], [0; 2], 100_000_000_000), None);
        assert_eq!(interval_bytes([100, 50], [100, 50], 1_000_000_000), Some(0));
    }
}
