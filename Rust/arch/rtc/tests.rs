use super::*;
use crate::fixture;
use std::sync::{Arc, Barrier, atomic::Ordering};

fn reset() {
    let _guard = owner::rtc_guard();
    fixture::reset();
    unsafe {
        (*STATE.0.get()).invalidate();
    }
}
fn date(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8, weekday: u8) -> DateTime {
    DateTime {
        valid: true,
        year,
        month,
        day,
        hour,
        minute,
        second,
        weekday,
        century_register: 0x32,
        century_source: 1,
    }
}
#[test]
fn calendar_decodes_modes_rejects_invalid_fields_and_carries_real_dates() {
    let mut raw = Raw {
        second: 0x59,
        minute: 0x59,
        hour: 0x11,
        weekday: 0,
        day: 0x28,
        month: 0x02,
        year: 0x00,
        century: 0x20,
        status: 0,
    };
    assert_eq!(
        calendar::decode_raw(raw, 0x32, 1).unwrap(),
        date(2000, 2, 28, 11, 59, 59, 1)
    );
    assert_eq!(
        seconds_since_midnight(calendar::decode_raw(raw, 0x32, 1).unwrap()),
        43199
    );
    raw.hour = 0x91;
    let late = calendar::decode_raw(raw, 0x32, 1).unwrap();
    assert_eq!(late, date(2000, 2, 28, 23, 59, 59, 1));
    assert_eq!(
        calendar::advance(late, 1).unwrap(),
        date(2000, 2, 29, 0, 0, 0, 2)
    );
    raw.hour = 0x12;
    assert_eq!(calendar::decode_raw(raw, 0x32, 1).unwrap().hour, 0);
    raw.hour = 0x92;
    assert_eq!(calendar::decode_raw(raw, 0x32, 1).unwrap().hour, 12);
    raw.second = 0x6a;
    assert!(calendar::decode_raw(raw, 0x32, 1).is_none());
    raw.second = 0;
    raw.day = 0x31;
    assert!(calendar::decode_raw(raw, 0x32, 1).is_none());
    assert_eq!(
        calendar::advance(date(2031, 12, 31, 23, 59, 59, 3), 1).unwrap(),
        date(2032, 1, 1, 0, 0, 0, 4)
    );
    assert!(calendar::advance(date(2099, 12, 31, 23, 59, 59, 4), 1).is_none());
    assert!(calendar::advance(late, 301).is_none());
    raw.day = 1;
    raw.month = 1;
    raw.year = 0x80;
    raw.century = 0xff;
    raw.hour = 0x12;
    let pivot = calendar::decode_raw(raw, 0x32, 2).unwrap();
    assert_eq!(
        (pivot.year, pivot.weekday, pivot.century_source),
        (1980, 2, 0)
    );
}
#[test]
fn cache_expires_fallback_and_never_reanchors_an_old_calendar_generation() {
    let a = date(2031, 12, 31, 23, 59, 59, 3);
    let b = date(2000, 2, 29, 1, 2, 3, 2);
    let mut cache = cache::Cache::EMPTY;
    assert!(cache.due(Some(0), 0x32));
    cache.publish(Some(0), 0x32, Some(a));
    assert!(!cache.due(Some(59_000_000_000), 0x32));
    assert_eq!(
        cache.current(Some(1_000_000_000)),
        date(2032, 1, 1, 0, 0, 0, 4)
    );
    assert!(cache.due(Some(60_000_000_000), 0x32));
    cache.publish(Some(60_000_000_000), 0x32, None);
    assert!(cache.current(Some(300_000_000_000)).valid);
    assert!(!cache.current(Some(300_000_000_001)).valid);
    cache.publish(Some(400_000_000_000), 0x32, Some(b));
    assert_eq!(cache.current(Some(400_000_000_000)), b);
    assert!(cache.due(Some(399_000_000_000), 0x32));
    assert!(!cache.current(Some(400_000_000_000)).valid);
    cache.publish(Some(400_000_000_000), 0x32, Some(a));
    assert!(cache.due(None, 0x32));
    assert!(!cache.current(None).valid);
    cache.publish(None, 0x32, Some(b));
    assert_eq!(cache.current(None), b);
    assert!(cache.due(Some(500_000_000_000), 0));
    assert!(!cache.current(Some(500_000_000_000)).valid);
}
#[test]
fn hot_reads_do_no_cmos_io_and_monotonic_seconds_advance_the_same_snapshot() {
    reset();
    assert_eq!(read_date_time(0x32), date(2026, 10, 25, 12, 34, 56, 0));
    let reads = fixture::PORTS.lock().unwrap().reads;
    fixture::CLOCK.store(1_000_000_000, Ordering::Relaxed);
    for _ in 0..1000 {
        assert_eq!(read_date_time(0x32), date(2026, 10, 25, 12, 34, 57, 0));
    }
    assert_eq!(fixture::PORTS.lock().unwrap().reads, reads);
}
#[test]
fn uip_and_changed_century_never_publish_the_last_unstable_sample() {
    reset();
    fixture::PORTS.lock().unwrap().registers[0xa] = 0x80;
    assert!(!read_date_time(0x32).valid);
    let reads = fixture::PORTS.lock().unwrap().reads;
    assert!(reads <= 3);
    for _ in 0..100 {
        assert!(!read_date_time(0x32).valid);
    }
    assert_eq!(fixture::PORTS.lock().unwrap().reads, reads);
    {
        let mut p = fixture::PORTS.lock().unwrap();
        p.registers[0xa] = 0;
        p.change_century = true;
    }
    fixture::CLOCK.store(10_000_000, Ordering::Relaxed);
    assert!(!read_date_time(0x32).valid);
    fixture::PORTS.lock().unwrap().registers[0x32] = 20;
    fixture::CLOCK.store(20_000_000, Ordering::Relaxed);
    assert!(read_date_time(0x32).valid);
    reset();
    fixture::PORTS.lock().unwrap().registers[0xd] = 0;
    assert!(!read_date_time(0x32).valid);
}
#[test]
fn set_freezes_and_verifies_all_fields_and_invalidates_after_uncertain_writes() {
    reset();
    let original = read_date_time(0x32);
    let wanted = date(2036, 2, 29, 1, 2, 3, 5);
    let mut invalid = wanted;
    invalid.day = 30;
    assert!(!write_date_time(0x32, invalid));
    assert_eq!(fixture::PORTS.lock().unwrap().calendar_writes, 0);
    fixture::PORTS.lock().unwrap().registers[0xa] = 0x80;
    assert!(!write_date_time(0x32, wanted));
    assert_eq!(read_date_time(0x32), original);
    {
        let mut p = fixture::PORTS.lock().unwrap();
        p.registers[0xa] = 0;
        p.ignore_write = Some(8);
    }
    assert!(!write_date_time(0x32, wanted));
    assert_eq!(fixture::PORTS.lock().unwrap().registers[0xb], 6);
    assert_eq!(read_date_time(0x32), date(2036, 10, 29, 1, 2, 3, 3));
    fixture::PORTS.lock().unwrap().ignore_write = None;
    assert!(write_date_time(0x32, wanted));
    assert_eq!(read_date_time(0x32), wanted);
    assert_eq!(fixture::PORTS.lock().unwrap().registers[0xb], 6);
    fixture::PORTS.lock().unwrap().registers[0xb] = 0x16;
    assert!(write_date_time(0x32, wanted));
    assert_eq!(fixture::PORTS.lock().unwrap().registers[0xb], 0x16);
}
#[test]
fn four_concurrent_readers_and_setters_observe_only_whole_calendar_generations() {
    reset();
    let a = date(2028, 2, 29, 1, 2, 3, 2);
    let b = date(2031, 12, 31, 23, 57, 59, 3);
    assert!(write_date_time(0x32, a));
    let gate = Arc::new(Barrier::new(4));
    std::thread::scope(|scope| {
        for index in 0..4 {
            let gate = gate.clone();
            scope.spawn(move || {
                gate.wait();
                if index < 2 {
                    for _ in 0..200 {
                        assert!(write_date_time(0x32, if index == 0 { a } else { b }));
                    }
                } else {
                    for _ in 0..1000 {
                        let got = read_date_time(0x32);
                        assert!(got == a || got == b);
                    }
                }
            });
        }
    });
}
