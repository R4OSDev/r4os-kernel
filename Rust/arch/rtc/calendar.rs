//! Kernel's early calendar representation; no I/O or allocation.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub valid: bool,
    pub month: u8,
    pub day: u8,
    /// Public R4SYS convention: Sunday=0 through Saturday=6.
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub century_register: u8,
    pub century_source: u8,
}
impl DateTime {
    pub const EMPTY: Self = Self {
        year: 1980,
        valid: false,
        month: 1,
        day: 1,
        weekday: 0,
        hour: 0,
        minute: 0,
        second: 0,
        century_register: 0,
        century_source: 0,
    };
}
impl Default for DateTime {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Raw {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub weekday: u8,
    pub day: u8,
    pub month: u8,
    pub year: u8,
    pub century: u8,
    pub status: u8,
}
pub fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => 0,
    }
}
pub fn valid_date_time(value: DateTime) -> bool {
    value.valid
        && (1980..=2099).contains(&value.year)
        && (1..=12).contains(&value.month)
        && value.day >= 1
        && value.day <= days_in_month(value.year, value.month)
        && value.hour <= 23
        && value.minute <= 59
        && value.second <= 59
}
pub fn seconds_since_midnight(value: DateTime) -> u32 {
    u32::from(value.hour) * 3600 + u32::from(value.minute) * 60 + u32::from(value.second)
}
/// Called only for a validated date. January 1, 1980 was Tuesday.
pub(super) fn weekday(value: DateTime) -> u8 {
    let mut days = 2u32;
    for year in 1980..value.year {
        days += 337 + u32::from(days_in_month(year, 2));
    }
    for month in 1..value.month {
        days += u32::from(days_in_month(value.year, month));
    }
    ((days + u32::from(value.day) - 1) % 7) as u8
}
fn decode(value: u8, binary: bool) -> Option<u8> {
    if binary {
        Some(value)
    } else if value & 15 <= 9 && value >> 4 <= 9 {
        Some((value >> 4) * 10 + (value & 15))
    } else {
        None
    }
}
pub(super) fn encode(value: u8, binary: bool) -> u8 {
    if binary {
        value
    } else {
        ((value / 10) << 4) | (value % 10)
    }
}
pub(super) fn decode_raw(raw: Raw, register: u8, source: u8) -> Option<DateTime> {
    let binary = raw.status & 4 != 0;
    let year_two = decode(raw.year, binary)?;
    if year_two > 99 {
        return None;
    }
    let century = decode(raw.century, binary).filter(|c| (19..=99).contains(c));
    let year = match century {
        Some(c) => u16::from(c) * 100 + u16::from(year_two),
        None => (if year_two >= 80 { 1900 } else { 2000 }) + u16::from(year_two),
    };
    let mut hour = decode(raw.hour & 0x7f, binary)?;
    if raw.status & 2 == 0 {
        if !(1..=12).contains(&hour) {
            return None;
        }
        hour = hour % 12 + if raw.hour & 0x80 != 0 { 12 } else { 0 };
    } else if raw.hour & 0x80 != 0 {
        return None;
    }
    let mut value = DateTime {
        valid: true,
        year,
        month: decode(raw.month, binary)?,
        day: decode(raw.day, binary)?,
        hour,
        minute: decode(raw.minute, binary)?,
        second: decode(raw.second, binary)?,
        century_register: if century.is_some() { register } else { 0 },
        century_source: if century.is_some() { source } else { 0 },
        ..DateTime::EMPTY
    };
    if !valid_date_time(value) {
        return None;
    }
    value.weekday = weekday(value);
    Some(value)
}
/// Bounded extrapolation only. The cache admits at most five minutes.
pub(super) fn advance(mut value: DateTime, elapsed_seconds: u64) -> Option<DateTime> {
    if !valid_date_time(value) || elapsed_seconds > 300 {
        return None;
    }
    let seconds = u64::from(seconds_since_midnight(value)) + elapsed_seconds;
    value.hour = ((seconds % 86400) / 3600) as u8;
    value.minute = ((seconds % 3600) / 60) as u8;
    value.second = (seconds % 60) as u8;
    if seconds >= 86400 {
        value.day += 1;
        value.weekday = (value.weekday + 1) % 7;
        if value.day > days_in_month(value.year, value.month) {
            value.day = 1;
            value.month += 1;
            if value.month > 12 {
                value.month = 1;
                value.year += 1;
            }
        }
    }
    valid_date_time(value).then_some(value)
}
const _: () = {
    assert!(core::mem::size_of::<DateTime>() == 12);
    assert!(core::mem::offset_of!(DateTime, valid) == 2);
    assert!(core::mem::offset_of!(DateTime, century_source) == 10);
};
