//! Public wall-clock state mapping; RTC owns calendar and CMOS work. The
//! immutable native ACPI snapshot supplies the single FADT century byte.
use super::{acpi, timer};
use crate::{arch::rtc, contract::TimeState};
fn century() -> u8 {
    acpi::snapshot().fadt_century
}
fn date_time(value: TimeState) -> rtc::DateTime {
    rtc::DateTime {
        valid: value.valid != 0,
        year: value.year,
        month: value.month,
        day: value.day,
        weekday: value.weekday,
        hour: value.hour,
        minute: value.minute,
        second: value.second,
        ..rtc::DateTime::default()
    }
}
/// # Safety
/// Caller owns one aligned writable Contract TimeState until return; the
/// established RTC/ACPI caller context admits this bounded CMOS sampling.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_time_state(output: *mut TimeState) {
    let now = rtc::read_date_time(century());
    let value = TimeState {
        valid: u8::from(now.valid),
        century_source: now.century_source,
        weekday: now.weekday,
        year: now.year,
        month: now.month,
        day: now.day,
        hour: now.hour,
        minute: now.minute,
        second: now.second,
        seconds_since_midnight: rtc::seconds_since_midnight(now),
        monotonic_ticks: timer::tick_count(),
        monotonic_hz: timer::frequency(),
        monotonic_backend: timer::active_backend(),
        ..TimeState::default()
    };
    unsafe { output.write(value) };
}
/// # Safety
/// Input is a resident readable Contract TimeState for the whole call. RTC
/// port/NMI context and exclusive hardware mutation are retained by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_time_set_state(input: *const TimeState) -> i32 {
    let value = date_time(unsafe { input.read() });
    if !rtc::valid_date_time(value) {
        return -1;
    }
    if rtc::write_date_time(century(), value) {
        0
    } else {
        -2
    }
}
const _: () = {
    assert!(core::mem::size_of::<TimeState>() == 32);
};

pub(crate) fn log_status() {
    use crate::boot::log::{r4os_rust_bootlog_dec as dec, text as puts};
    fn two(value: u8) {
        if value < 10 {
            crate::boot::log::text(b"0");
        }
        crate::boot::log::r4os_rust_bootlog_dec(u64::from(value));
    }
    let now = rtc::read_date_time(century());
    puts(b"[TIME] rtc=");
    two(now.day);
    puts(b"-");
    two(now.month);
    puts(b"-");
    dec(u64::from(now.year));
    puts(b" ");
    two(now.hour);
    puts(b":");
    two(now.minute);
    puts(b":");
    two(now.second);
    puts(b" century=");
    puts(match now.century_source {
        0 => b"pivot",
        1 => b"fadt",
        2 => b"fallback",
        _ => unreachable!(),
    });
    puts(b" monotonic=");
    let clock = super::monotonic::snapshot();
    puts(match clock.source {
        0 => b"unavailable",
        1 => b"TSC",
        2 => b"HPET main counter",
        3 => timer::backend_name(),
        _ => unreachable!(),
    });
    puts(b" event=");
    puts(timer::backend_name());
    let hardware = super::clocksource::snapshot();
    puts(b" source_hz=");
    dec(hardware.frequency_hz);
    puts(b" tsc_invariant=");
    puts(if hardware.flags & 2 != 0 {
        b"yes"
    } else {
        b"no"
    });
    puts(b" hpet_calibrated=");
    puts(if hardware.flags & 4 != 0 {
        b"yes"
    } else {
        b"no"
    });
    puts(b" calibration_ppm=");
    dec(hardware.calibration_error_ppm);
    puts(b" fallback=");
    puts(super::clocksource::fallback_name(hardware.fallback_reason));
    puts(b"\r\n");
}
