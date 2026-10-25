//! NTFS operation-time conversion. No mount-wide timestamp cache: each
//! timestamp-writing adapter operation admits one current canonical calendar.
use crate::{arch::rtc, platform::time};
pub(super) fn now_filetime() -> Option<u64> {
    filetime(time::calendar())
}
fn filetime(now: rtc::DateTime) -> Option<u64> {
    if !rtc::valid_date_time(now) {
        return None;
    }
    let y = i64::from(now.year) - i64::from(now.month <= 2);
    let era = (if y >= 0 { y } else { y - 399 }).div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (i64::from(now.month) + if now.month > 2 { -3 } else { 9 }) + 2).div_euclid(5)
        + i64::from(now.day)
        - 1;
    let days =
        era * 146097 + yoe * 365 + yoe.div_euclid(4) - yoe.div_euclid(100) + doy - 719468 + 134774;
    if days < 0 {
        return None;
    }
    Some(
        (days as u64 * 86400
            + u64::from(now.hour) * 3600
            + u64::from(now.minute) * 60
            + u64::from(now.second))
            * 10_000_000,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn date(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> rtc::DateTime {
        rtc::DateTime {
            valid: true,
            year,
            month,
            day,
            hour,
            minute,
            second,
            ..rtc::DateTime::default()
        }
    }
    #[test]
    fn canonical_calendar_maps_to_exact_filetime_epoch_and_leap_boundaries() {
        // Independent FILETIME constants (100ns since 1601-01-01 UTC).
        for (value, expected) in [
            (date(1980, 1, 1, 0, 0, 0), 119_600_064_000_000_000),
            (date(2000, 2, 29, 23, 59, 59), 125_963_423_990_000_000),
            (date(2000, 3, 1, 0, 0, 0), 125_963_424_000_000_000),
            (date(2026, 10, 25, 12, 34, 56), 134_374_052_960_000_000),
            (date(2099, 12, 31, 23, 59, 59), 157_469_183_990_000_000),
        ] {
            assert_eq!(filetime(value), Some(expected));
        }
    }
    #[test]
    fn unavailable_or_invalid_calendar_never_becomes_a_zero_timestamp() {
        assert_eq!(filetime(rtc::DateTime::default()), None);
        for value in [
            date(2026, 2, 29, 0, 0, 0),
            date(2026, 13, 1, 0, 0, 0),
            date(2026, 1, 0, 0, 0, 0),
            date(2026, 1, 1, 24, 0, 0),
            date(2026, 1, 1, 0, 60, 0),
            date(2026, 1, 1, 0, 0, 60),
            date(1979, 12, 31, 23, 59, 59),
            date(2100, 1, 1, 0, 0, 0),
        ] {
            assert_eq!(filetime(value), None);
        }
    }
    #[test]
    fn new_operations_follow_the_canonical_clock_and_its_invalidity() {
        use crate::fixture;
        use core::sync::atomic::Ordering::Relaxed;
        fixture::reset();
        let a = date(2000, 2, 29, 23, 59, 59);
        assert!(rtc::write_date_time(0x32, a));
        let operation = now_filetime().unwrap();
        fixture::CLOCK.store(1_000_000_000, Relaxed);
        assert_eq!(now_filetime(), Some(operation + 10_000_000));
        let b = date(2026, 10, 25, 12, 34, 56);
        assert!(rtc::write_date_time(0x32, b));
        assert_eq!(now_filetime(), filetime(b));
        // A copied admitted operation stamp stays exact; the next operation
        // must consult the canonical owner and may not reuse this local copy.
        assert_eq!(operation, 125_963_423_990_000_000);
        fixture::PORTS.lock().unwrap().registers[0x0d] = 0;
        fixture::CLOCK.store(302_000_000_000, Relaxed);
        assert_eq!(now_filetime(), None);
    }
}
