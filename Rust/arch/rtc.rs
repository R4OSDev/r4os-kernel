//! Original bounded CMOS sampling and calendar policy. Port admission and
//! NMI/IRQ context remain with the existing platform caller, without a new lock.
use super::io;
const CENTURY_FALLBACK: u8 = 0x32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DateTime {
    pub year: u16,
    pub valid: bool,
    pub month: u8,
    pub day: u8,
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub century_register: u8,
    pub century_source: u8,
}
impl Default for DateTime {
    fn default() -> Self {
        Self {
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
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Raw {
    second: u8,
    minute: u8,
    hour: u8,
    weekday: u8,
    day: u8,
    month: u8,
    year: u8,
    century: u8,
    status: u8,
}
impl Default for Raw {
    fn default() -> Self {
        Self {
            second: 0,
            minute: 0,
            hour: 0,
            weekday: 0,
            day: 1,
            month: 1,
            year: 0,
            century: 0,
            status: 0,
        }
    }
}
fn read(reg: u8) -> u8 {
    unsafe {
        io::outb(0x70, reg | 0x80);
        io::outb(0x80, 0);
        io::inb(0x71)
    }
}
fn write(reg: u8, value: u8) {
    unsafe {
        io::outb(0x70, reg | 0x80);
        io::outb(0x80, 0);
        io::outb(0x71, value);
        io::outb(0x80, 0);
    }
}
fn wait_stable() {
    for _ in 0..10000 {
        if read(0x0a) & 0x80 == 0 {
            return;
        }
    }
}
fn raw(century: u8) -> Raw {
    Raw {
        second: read(0),
        minute: read(2),
        hour: read(4),
        weekday: read(6),
        day: read(7),
        month: read(8),
        year: read(9),
        century: if century != 0 { read(century) } else { 0 },
        status: read(0x0b),
    }
}
fn raw_stable(century: u8) -> Raw {
    let mut last = Raw::default();
    for _ in 0..4 {
        wait_stable();
        let a = raw(century);
        wait_stable();
        let b = raw(century);
        if a == b {
            return b;
        }
        last = b;
    }
    last
}
fn normalize(value: u8, binary: bool) -> u8 {
    if binary {
        value
    } else {
        (value >> 4) * 10 + (value & 15)
    }
}
fn encode(value: u8, binary: bool) -> u8 {
    if binary {
        value
    } else {
        ((value / 10) << 4) | (value % 10)
    }
}
pub fn read_date_time(century_register: u8) -> DateTime {
    let raw = raw_stable(century_register);
    let binary = raw.status & 4 != 0;
    let mut second = normalize(raw.second, binary);
    let mut minute = normalize(raw.minute, binary);
    let pm = raw.hour & 0x80 != 0;
    let mut hour = normalize(raw.hour & 0x7f, binary);
    let mut weekday = normalize(raw.weekday, binary);
    let mut day = normalize(raw.day, binary);
    let mut month = normalize(raw.month, binary);
    let year_two = normalize(raw.year, binary);
    if raw.status & 2 == 0 {
        if pm && hour < 12 {
            hour += 12;
        }
        if !pm && hour == 12 {
            hour = 0;
        }
    }
    if second > 59 {
        second = 0;
    }
    if minute > 59 {
        minute = 0;
    }
    if hour > 23 {
        hour = 0;
    }
    if weekday > 7 {
        weekday = 0;
    }
    if !(1..=31).contains(&day) {
        day = 1;
    }
    if !(1..=12).contains(&month) {
        month = 1;
    }
    let mut year = 0u16;
    let mut source = 0u8;
    let mut register = 0u8;
    if century_register != 0 {
        let century = normalize(raw.century, binary);
        if (19..=99).contains(&century) {
            year = u16::from(century) * 100 + u16::from(year_two);
            source = 1;
            register = century_register;
        }
    }
    if year == 0 && century_register == 0 {
        let century = normalize(read(CENTURY_FALLBACK), binary);
        if (19..=99).contains(&century) {
            year = u16::from(century) * 100 + u16::from(year_two);
            source = 2;
            register = CENTURY_FALLBACK;
        }
    }
    if year == 0 {
        year = if year_two >= 80 {
            1900 + u16::from(year_two)
        } else {
            2000 + u16::from(year_two)
        };
    }
    DateTime {
        year,
        valid: (1980..=2099).contains(&year)
            && (1..=12).contains(&month)
            && (1..=31).contains(&day),
        month,
        day,
        weekday,
        hour,
        minute,
        second,
        century_register: register,
        century_source: source,
    }
}
pub fn seconds_since_midnight(value: DateTime) -> u32 {
    u32::from(value.hour) * 3600 + u32::from(value.minute) * 60 + u32::from(value.second)
}
pub fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) {
                29
            } else {
                28
            }
        }
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
pub fn write_date_time(century_register: u8, value: DateTime) -> bool {
    if !valid_date_time(value) {
        return false;
    }
    wait_stable();
    let status = read(0x0b);
    let binary = status & 4 != 0;
    let hour = if status & 2 != 0 {
        encode(value.hour, binary)
    } else {
        let pm = value.hour >= 12;
        let mut hour = value.hour % 12;
        if hour == 0 {
            hour = 12;
        }
        let mut encoded = encode(hour, binary);
        if pm {
            encoded |= 0x80;
        }
        encoded
    };
    let weekday = if value.weekday <= 6 {
        value.weekday + 1
    } else {
        value.weekday
    };
    let century = if century_register != 0 {
        century_register
    } else {
        CENTURY_FALLBACK
    };
    write(0, encode(value.second, binary));
    write(2, encode(value.minute, binary));
    write(4, hour);
    write(6, encode(weekday, binary));
    write(7, encode(value.day, binary));
    write(8, encode(value.month, binary));
    write(9, encode((value.year % 100) as u8, binary));
    write(century, encode((value.year / 100) as u8, binary));
    true
}
const _: () = {
    assert!(core::mem::size_of::<DateTime>() == 12);
    assert!(core::mem::offset_of!(DateTime, valid) == 2);
    assert!(core::mem::offset_of!(DateTime, century_source) == 10);
};
/// # Safety
/// Caller owns the CMOS port transaction and one aligned writable C output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_rtc_read(century: u8, output: *mut DateTime) {
    unsafe { output.write(read_date_time(century)) };
}
/// # Safety
/// Caller owns the CMOS port transaction and an initialized resident C input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_rtc_write(century: u8, value: *const DateTime) -> bool {
    write_date_time(century, unsafe { value.read() })
}
/// # Safety
/// Caller owns one initialized resident read-only C value until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_rtc_value(operation: u8, value: *const DateTime) -> u32 {
    let value = unsafe { value.read() };
    match operation {
        0 => seconds_since_midnight(value),
        1 => u32::from(valid_date_time(value)),
        _ => panic!("Invalid private calendar operation"),
    }
}
