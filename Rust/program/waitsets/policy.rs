//! Bounded source selection over the shared checked absolute budget.
pub(super) use super::super::deadline_policy::{Budget, ClockKey, Error, Sample, ticks};
pub(super) struct Selection {
    pub indices: [u8; 32],
    pub count: usize,
    pub more: bool,
    pub next: usize,
}
pub(super) fn select(mask: u32, slots: usize, capacity: usize, start: usize) -> Selection {
    assert!((1..=32).contains(&slots) && (1..=32).contains(&capacity) && start < slots);
    let mut result = Selection {
        indices: [0; 32],
        count: 0,
        more: false,
        next: start,
    };
    for step in 0..slots {
        let index = (start + step) % slots;
        if mask & (1 << index) == 0 {
            continue;
        }
        if result.count == capacity {
            result.more = true;
        } else {
            result.indices[result.count] = index as u8;
            result.count += 1;
            result.next = (index + 1) % slots;
        }
    }
    result
}
pub(super) fn select_ready(
    mask: u32,
    stop_mask: u32,
    slots: usize,
    capacity: usize,
    start: usize,
) -> Selection {
    let selected_mask = if stop_mask == 0 {
        mask
    } else {
        mask & stop_mask
    };
    let mut selected = select(selected_mask, slots, capacity, start);
    selected.more |= selected_mask != mask;
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    const KEY: ClockKey = ClockKey {
        source: 1,
        generation: 2,
        event_backend: 1,
        tick_hz: 333,
    };
    #[test]
    fn one_deadline_survives_wakes_and_refuses_clock_change_without_mutation() {
        let mut budget = Budget {
            kind: 1,
            key: KEY,
            absolute_ns: 120_000_000,
            last_ns: 10_000_000,
        };
        for (now, remaining) in [
            (20_000_000, 100_000_000),
            (119_000_000, 1_000_000),
            (120_000_000, 0),
            (130_000_000, 0),
        ] {
            assert_eq!(
                budget.remaining(Some(Sample {
                    key: KEY,
                    instant_ns: now
                })),
                Ok(Some(remaining))
            );
            assert_eq!(budget.absolute_ns, 120_000_000);
        }
        let before = budget.last_ns;
        assert_eq!(
            budget.remaining(Some(Sample {
                key: ClockKey {
                    generation: 3,
                    ..KEY
                },
                instant_ns: 140_000_000
            })),
            Err(Error::ClockChanged)
        );
        assert_eq!(budget.last_ns, before);
        assert_eq!(
            budget.remaining(Some(Sample {
                key: KEY,
                instant_ns: 120_000_000
            })),
            Err(Error::ClockRegressed)
        );
        assert_eq!(budget.last_ns, before);
        assert_eq!(budget.remaining(None), Err(Error::ClockUnavailable));
    }
    #[test]
    fn finite_maximum_and_poll_forever_keep_distinct_meanings() {
        let mut finite = Budget {
            kind: 1,
            key: KEY,
            absolute_ns: u64::MAX,
            last_ns: 0,
        };
        assert_eq!(
            finite.remaining(Some(Sample {
                key: KEY,
                instant_ns: 1
            })),
            Ok(Some(u64::MAX - 1))
        );
        for (kind, expected) in [(0, Some(0)), (2, None)] {
            let mut budget = Budget {
                kind,
                key: ClockKey::default(),
                absolute_ns: 0,
                last_ns: 0,
            };
            assert_eq!(budget.remaining(None), Ok(expected));
        }
        assert_eq!(
            Budget {
                kind: 2,
                key: KEY,
                absolute_ns: 0,
                last_ns: 0
            }
            .validate(),
            Err(Error::Invalid)
        );
    }
    #[test]
    fn logical_ticks_round_up_without_admitting_forever_by_overflow() {
        assert_eq!(ticks(0, 333), Ok(0));
        assert_eq!(ticks(1, 333), Ok(1));
        assert_eq!(ticks(3_003_004, 333), Ok(2));
        assert_eq!(ticks(120_000_000, 333), Ok(40));
        assert_eq!(ticks(u64::MAX, 1_000_000_000), Err(Error::Overflow));
        assert_eq!(ticks(u64::MAX, u32::MAX), Err(Error::Overflow));
        assert_eq!(ticks(1, 0), Err(Error::ClockUnavailable));
    }
    #[test]
    fn short_batches_rotate_across_every_ready_source_without_consuming_it() {
        let mask = (1 << 1) | (1 << 5) | (1 << 17) | (1 << 30);
        let first = select(mask, 32, 2, 31);
        assert_eq!(&first.indices[..first.count], &[1, 5]);
        assert!(first.more);
        let second = select(mask, 32, 2, first.next);
        assert_eq!(&second.indices[..second.count], &[17, 30]);
        assert!(second.more);
        let third = select(mask, 32, 2, second.next);
        assert_eq!(&third.indices[..third.count], &[1, 5]);
        let empty = select(0, 3, 1, 2);
        assert_eq!(empty.count, 0);
        assert!(!empty.more);
        assert_eq!(empty.next, 2);
    }
    #[test]
    fn stop_cannot_hide_behind_a_short_ready_batch() {
        let result = select_ready(0b1111, 0b1000, 4, 1, 0);
        assert_eq!(&result.indices[..result.count], &[3]);
        assert!(result.more);
        let ordinary = select_ready(0b1111, 0, 4, 2, 0);
        assert_eq!(&ordinary.indices[..ordinary.count], &[0, 1]);
        assert!(ordinary.more);
    }
}
