//! Canonical short RTC owner for CMOS index/data, calendar writes and cache.
//! No allocation, callback, wait or RuntimeGuard is acquired while held.
#[path = "rtc/cache.rs"]
mod cache;
#[path = "rtc/calendar.rs"]
mod calendar;
use super::io;
use crate::{memory::owner, platform::monotonic};
pub use calendar::{DateTime, seconds_since_midnight, valid_date_time};
use calendar::{Raw, encode};
use core::cell::UnsafeCell;
struct State(UnsafeCell<cache::Cache>);
// SAFETY: every cache/hardware operation holds the same resident RTC owner.
unsafe impl Sync for State {}
static STATE: State = State(UnsafeCell::new(cache::Cache::EMPTY));
const UIP: u8 = 0x80;
const SET: u8 = 0x80;
const UPDATE_IRQ: u8 = 0x10;
// Keep R4OS's existing NMI policy. Port70 is write-only on supported Intel
// profiles; reading it cannot recover an unknown previous index/NMI mask.
const INDEX_NMI_DISABLED: u8 = 0x80;
fn century(requested: u8) -> (u8, u8) {
    if (0x0e..=0x7f).contains(&requested) {
        (requested, 1)
    } else {
        (0x32, 2)
    }
}
fn read(_guard: &owner::Guard, reg: u8) -> u8 {
    unsafe {
        io::outb(0x70, reg | INDEX_NMI_DISABLED);
        io::outb(0x80, 0);
        io::inb(0x71)
    }
}
fn write(_guard: &owner::Guard, reg: u8, value: u8) {
    unsafe {
        io::outb(0x70, reg | INDEX_NMI_DISABLED);
        io::outb(0x80, 0);
        io::outb(0x71, value);
        io::outb(0x80, 0);
    }
}
fn raw(guard: &owner::Guard, century: u8) -> Raw {
    Raw {
        second: read(guard, 0),
        minute: read(guard, 2),
        hour: read(guard, 4),
        weekday: read(guard, 6),
        day: read(guard, 7),
        month: read(guard, 8),
        year: read(guard, 9),
        century: read(guard, century),
        status: read(guard, 0x0b),
    }
}
/// One bounded attempt; UIP never starts a spin/poll loop. First/last seconds,
/// both whole samples (including century/mode), and final UIP must agree.
fn sample(guard: &owner::Guard, register: u8, source: u8) -> Option<DateTime> {
    let second = read(guard, 0);
    if read(guard, 0x0a) & UIP != 0 {
        return None;
    }
    let a = raw(guard, register);
    if a.status & SET != 0 || a.second != second || read(guard, 0x0a) & UIP != 0 {
        return None;
    }
    let b = raw(guard, register);
    if a != b
        || b.second != read(guard, 0)
        || read(guard, 0x0a) & UIP != 0
        || read(guard, 0x0d) & 0x80 == 0
    {
        return None;
    }
    calendar::decode_raw(b, register, source)
}
pub fn read_date_time(requested_century: u8) -> DateTime {
    let guard = owner::rtc_guard();
    let now = monotonic::now_nanoseconds();
    if unsafe { (*STATE.0.get()).due(now, requested_century) } {
        let (register, source) = century(requested_century);
        let sampled = sample(&guard, register, source);
        let now = monotonic::now_nanoseconds();
        unsafe {
            (*STATE.0.get()).publish(now, requested_century, sampled);
        }
        return unsafe { (*STATE.0.get()).current(now) };
    }
    unsafe { (*STATE.0.get()).current(now) }
}
pub fn write_date_time(requested_century: u8, mut value: DateTime) -> bool {
    if !valid_date_time(value) {
        return false;
    }
    let guard = owner::rtc_guard();
    if read(&guard, 0x0a) & UIP != 0 || read(&guard, 0x0d) & 0x80 == 0 {
        return false;
    }
    let status = read(&guard, 0x0b);
    if status & SET != 0 {
        return false;
    }
    let (register, source) = century(requested_century);
    let binary = status & 4 != 0;
    value.weekday = calendar::weekday(value);
    value.century_register = register;
    value.century_source = source;
    let hour = if status & 2 != 0 {
        encode(value.hour, binary)
    } else {
        let hour = value.hour % 12;
        encode(if hour == 0 { 12 } else { hour }, binary) | if value.hour >= 12 { 0x80 } else { 0 }
    };
    let expected = Raw {
        second: encode(value.second, binary),
        minute: encode(value.minute, binary),
        hour,
        weekday: encode(value.weekday + 1, binary),
        day: encode(value.day, binary),
        month: encode(value.month, binary),
        year: encode((value.year % 100) as u8, binary),
        century: encode((value.year / 100) as u8, binary),
        // MC146818/QEMU clears UIE in SET mode. Mask it explicitly so the
        // frozen readback also agrees on PCHs that retain the writable bit.
        status: (status | SET) & !UPDATE_IRQ,
    };
    // After the first hardware mutation a failed write cannot leave an older
    // cached generation valid. Restore the original control byte on every exit.
    unsafe {
        (*STATE.0.get()).invalidate();
    }
    write(&guard, 0x0b, expected.status);
    let frozen = read(&guard, 0x0b) == expected.status && read(&guard, 0x0a) & UIP == 0;
    let mut verified = false;
    if frozen {
        write(&guard, 0, expected.second);
        write(&guard, 2, expected.minute);
        write(&guard, 4, expected.hour);
        write(&guard, 6, expected.weekday);
        write(&guard, 7, expected.day);
        write(&guard, 8, expected.month);
        write(&guard, 9, expected.year);
        write(&guard, register, expected.century);
        verified = raw(&guard, register) == expected;
    }
    write(&guard, 0x0b, status);
    verified &= read(&guard, 0x0b) == status;
    if verified {
        let now = monotonic::now_nanoseconds();
        unsafe {
            (*STATE.0.get()).publish(now, requested_century, Some(value));
        }
    }
    verified
}

#[cfg(test)]
#[path = "rtc/tests.rs"]
mod tests;
