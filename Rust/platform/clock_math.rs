//! Original clock qualification, rational conversion and deadline policies.
//! Caller-owned C values only; no clock origin or hardware state lives here.
pub const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;
const _: () = {
    assert!(core::mem::size_of::<Rate>() == 16);
    assert!(core::mem::size_of::<FixedScale>() == 16);
    assert!(core::mem::offset_of!(FixedScale, shift) == 8);
    assert!(core::mem::size_of::<OrderedDurations>() == 32);
    assert!(core::mem::offset_of!(OrderedDurations, has_first) == 28);
};

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rate {
    pub numerator: u64,
    pub denominator: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FixedScale {
    pub multiplier: u64,
    pub shift: u8,
}
impl FixedScale {
    pub fn init_floor(numerator: u64, denominator: u64) -> Self {
        if numerator == 0 || denominator == 0 {
            return Self::default();
        }
        let mut shift = 63;
        loop {
            let scaled = (u128::from(numerator) << shift) / u128::from(denominator);
            if scaled != 0 && scaled <= u128::from(u64::MAX) {
                return Self {
                    multiplier: scaled as u64,
                    shift,
                };
            }
            if shift == 0 {
                return Self::default();
            }
            shift -= 1;
        }
    }
    pub fn valid(self) -> bool {
        self.multiplier != 0
    }
    pub fn apply(self, value: u64) -> u64 {
        if !self.valid() || value == 0 {
            return 0;
        }
        clamp64((u128::from(value) * u128::from(self.multiplier)) >> self.shift)
    }
    /// First raw counter whose exact existing floor scale reaches `value`.
    /// Used to preserve an absolute event deadline across both clock scales.
    pub(crate) fn invert_ceil(self, value: u64) -> Option<u64> {
        if !self.valid() || self.shift > 63 {
            return None;
        }
        let scaled = u128::from(value) << self.shift;
        u64::try_from(scaled.div_ceil(u128::from(self.multiplier))).ok()
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OrderedDurations {
    pub first: u64,
    pub last: u64,
    pub total: u64,
    pub unavailable: u32,
    pub has_first: bool,
    pub has_last: bool,
}
impl OrderedDurations {
    pub fn add(&mut self, start: Option<u64>, end: Option<u64>) {
        let Some((start, end)) = start.zip(end) else {
            self.unavailable = self.unavailable.saturating_add(1);
            return;
        };
        if end < start || (self.has_last && start < self.last) {
            self.unavailable = self.unavailable.saturating_add(1);
            return;
        }
        if !self.has_first {
            self.first = start;
            self.has_first = true;
        }
        self.last = end;
        self.has_last = true;
        self.total = self.total.saturating_add(end - start);
    }
    pub fn valid(self) -> bool {
        self.has_first && self.has_last && self.unavailable == 0
    }
}
fn clamp64(value: u128) -> u64 {
    value.min(u128::from(u64::MAX)) as u64
}
pub fn choose_hardware_clock(flags: u8) -> u8 {
    let present = flags & 1 != 0;
    let frequency = flags & 2 != 0;
    let invariant = flags & 4 != 0;
    let hpet = flags & 8 != 0;
    let calibrated = flags & 16 != 0;
    let offsets = flags & 32 != 0;
    if present && frequency && offsets && (invariant || calibrated) {
        1
    } else if hpet {
        2
    } else {
        0
    }
}
pub fn frequency_error_ppm(measured: u64, expected: u64) -> u64 {
    if expected == 0 {
        return u64::MAX;
    }
    clamp64(u128::from(measured.abs_diff(expected)) * 1_000_000 / u128::from(expected))
}
pub fn signed_counter_correction(expected: u64, observed: u64) -> Option<i64> {
    i64::try_from(i128::from(expected) - i128::from(observed)).ok()
}
pub fn apply_signed_correction(raw: u64, correction: i64) -> u64 {
    if correction >= 0 {
        raw.wrapping_add(correction as u64)
    } else {
        raw.wrapping_sub(correction.unsigned_abs())
    }
}
pub fn counter_advanced(previous: u64, current: u64) -> bool {
    current.wrapping_sub(previous) < 1 << 63
}
pub fn scale_floor(value: u64, numerator: u64, denominator: u64) -> u64 {
    if value == 0 || numerator == 0 || denominator == 0 {
        return 0;
    }
    clamp64(u128::from(value) * u128::from(numerator) / u128::from(denominator))
}
pub fn scale_ceil(value: u64, numerator: u64, denominator: u64) -> u64 {
    if value == 0 || numerator == 0 || denominator == 0 {
        return 0;
    }
    let product = u128::from(value) * u128::from(numerator);
    clamp64((product + u128::from(denominator) - 1) / u128::from(denominator))
}
pub fn finite_deadline(now: u64, duration: u64) -> u64 {
    let max_finite = u64::MAX - 1;
    if duration >= max_finite.saturating_sub(now) {
        max_finite
    } else {
        now + duration
    }
}
pub fn bounded_deadline_delta(now: u64, deadline: u64, maximum: u64) -> u64 {
    if maximum == 0 {
        0
    } else if deadline <= now {
        1
    } else {
        (deadline - now).min(maximum)
    }
}
pub fn rate_to_nanoseconds(events: u64, numerator: u64, denominator: u64) -> u64 {
    if numerator == 0 || denominator == 0 {
        return 0;
    }
    let scale = u128::from(NANOSECONDS_PER_SECOND) * u128::from(denominator);
    clamp64(u128::from(events) * scale / u128::from(numerator))
}
pub fn resolution_nanoseconds(numerator: u64, denominator: u64) -> u64 {
    if numerator == 0 || denominator == 0 {
        return 0;
    }
    let scaled = u128::from(NANOSECONDS_PER_SECOND) * u128::from(denominator);
    clamp64((scaled + u128::from(numerator) - 1) / u128::from(numerator))
}
pub fn rounded_rate(numerator: u64, denominator: u64) -> u32 {
    if numerator == 0 || denominator == 0 {
        return 0;
    }
    let result = (u128::from(numerator) + u128::from(denominator / 2)) / u128::from(denominator);
    result.min(u128::from(u32::MAX)) as u32
}
pub fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}
pub fn reduced_rate(numerator: u64, denominator: u64) -> Rate {
    if numerator == 0 || denominator == 0 {
        return Rate::default();
    }
    let divisor = gcd(numerator, denominator);
    Rate {
        numerator: numerator / divisor,
        denominator: denominator / divisor,
    }
}
pub fn extend_counter32(published: u64, low: u32) -> u64 {
    let half = 1 << 31;
    let mut candidate = (published & !u64::from(u32::MAX)) | u64::from(low);
    if candidate < published {
        if published - candidate > half {
            candidate = candidate.saturating_add(1 << 32);
        } else {
            return published;
        }
    } else if candidate - published > half {
        return published;
    }
    candidate
}

/// A quarter-wrap observation interval, expressed in actual event ticks.
/// Two complete ticks leave room for rounding; no missing wrap is inferred.
pub(crate) fn counter32_observation_ticks(frequency_hz: u64, event: Rate) -> Option<u64> {
    if frequency_hz == 0 || event.numerator == 0 || event.denominator == 0 {
        return None;
    }
    let ticks = ((1u128 << 30) * u128::from(event.numerator))
        / (u128::from(frequency_hz) * u128::from(event.denominator));
    u64::try_from(ticks).ok().filter(|ticks| *ticks >= 2)
}

#[cfg(test)]
mod counter32_tests {
    use super::*;

    #[test]
    fn observation_uses_actual_frequency_and_event_rate() {
        let event = Rate {
            numerator: 1000,
            denominator: 1,
        };
        for (hz, ticks) in [
            (10_000_000, 107374),
            (100_000_000, 10737),
            (1_000_000_000, 1073),
        ] {
            assert_eq!(counter32_observation_ticks(hz, event), Some(ticks));
        }
        assert_eq!(
            counter32_observation_ticks(
                100_000_000,
                Rate {
                    numerator: 1_193_182,
                    denominator: 1193
                }
            ),
            Some(10739)
        );
    }

    #[test]
    fn unknown_or_insufficient_observation_cadence_is_unqualified() {
        for (hz, rate) in [
            (
                0,
                Rate {
                    numerator: 1000,
                    denominator: 1,
                },
            ),
            (
                100_000_000,
                Rate {
                    numerator: 0,
                    denominator: 1,
                },
            ),
            (
                100_000_000,
                Rate {
                    numerator: 1000,
                    denominator: 0,
                },
            ),
            (
                1_000_000_000_000,
                Rate {
                    numerator: 1000,
                    denominator: 1,
                },
            ),
            (
                u64::MAX,
                Rate {
                    numerator: u64::MAX,
                    denominator: u64::MAX,
                },
            ),
        ] {
            assert_eq!(counter32_observation_ticks(hz, rate), None);
        }
    }

    #[test]
    fn delivered_observations_preserve_half_full_and_multiple_wraps() {
        let mut actual = 256u64;
        let mut observed = actual;
        for _ in 0..37 {
            actual += (1 << 30) - 1;
            observed = extend_counter32(observed, actual as u32);
            assert_eq!(observed, actual);
        }
        assert!(actual > 9 * (1u64 << 32));
    }

    #[test]
    fn unobserved_long_gaps_are_not_a_reconstruction_oracle() {
        for elapsed in [(1u64 << 31) + 1, (1u64 << 32) + 100, 3 * (1u64 << 32) + 100] {
            let actual = 256 + elapsed;
            assert_ne!(extend_counter32(256, actual as u32), actual);
        }
    }
}
