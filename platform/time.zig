const r4x_api = @import("../program/r4x_api.zig");
const rtc = @import("../arch/x86_64/rtc.zig");
const acpi = @import("acpi.zig");
const bootlog = @import("../kernel/bootlog.zig");
const timer = @import("../kernel/timer.zig");

pub const WallClock = rtc.DateTime;

pub const State = r4x_api.TimeState;

pub fn wallClock() WallClock {
    return rtc.readDateTime(acpi.info().fadt_century);
}

pub fn state() State {
    const now = wallClock();
    return .{
        .valid = if (now.valid) 1 else 0,
        .century_source = centurySourceId(now.century_source),
        .weekday = now.weekday,
        .year = now.year,
        .month = now.month,
        .day = now.day,
        .hour = now.hour,
        .minute = now.minute,
        .second = now.second,
        .seconds_since_midnight = now.secondsSinceMidnight(),
        .monotonic_ticks = monotonicTicks(),
        .monotonic_hz = monotonicFrequency(),
        .monotonic_backend = monotonicBackendId(),
    };
}

pub fn setState(next: State) i32 {
    if (!validState(next)) return -1;
    const ok = rtc.writeDateTime(acpi.info().fadt_century, .{
        .valid = true,
        .year = next.year,
        .month = next.month,
        .day = next.day,
        .weekday = next.weekday,
        .hour = next.hour,
        .minute = next.minute,
        .second = next.second,
    });
    return if (ok) 0 else -2;
}

pub fn secondsSinceMidnight() u32 {
    return wallClock().secondsSinceMidnight();
}

pub fn monotonicTicks() u64 {
    return timer.tickCount();
}

pub fn monotonicFrequency() u32 {
    return timer.frequency();
}

pub fn monotonicBackendId() u32 {
    return switch (timer.activeBackend()) {
        .pit => 0,
        .hpet => 1,
        .lapic => 2,
    };
}

pub fn logStatus() void {
    const now = wallClock();
    bootlog.puts("[TIME] rtc=");
    bootlogDate(now);
    bootlog.puts(" ");
    bootlogTime(now);
    bootlog.puts(" century=");
    bootlog.puts(switch (now.century_source) {
        .none => "pivot",
        .fadt => "fadt",
        .fallback => "fallback",
    });
    bootlog.puts(" monotonic=");
    bootlog.puts(timer.backendName());
    bootlog.puts("\r\n");
}

fn bootlogDate(now: WallClock) void {
    bootlogPut2(now.day);
    bootlog.puts("-");
    bootlogPut2(now.month);
    bootlog.puts("-");
    bootlog.putDec(now.year);
}

fn bootlogTime(now: WallClock) void {
    bootlogPut2(now.hour);
    bootlog.puts(":");
    bootlogPut2(now.minute);
    bootlog.puts(":");
    bootlogPut2(now.second);
}

fn centurySourceId(source: rtc.CenturySource) u8 {
    return switch (source) {
        .none => 0,
        .fadt => 1,
        .fallback => 2,
    };
}

fn bootlogPut2(value: u64) void {
    if (value < 10) bootlog.puts("0");
    bootlog.putDec(value);
}

fn validState(value: State) bool {
    if (value.valid == 0) return false;
    if (value.year < 1980 or value.year > 2099) return false;
    if (value.month < 1 or value.month > 12) return false;
    if (value.day < 1 or value.day > daysInMonth(value.year, value.month)) return false;
    if (value.hour > 23 or value.minute > 59 or value.second > 59) return false;
    return true;
}

fn daysInMonth(year: u16, month: u8) u8 {
    return switch (month) {
        1, 3, 5, 7, 8, 10, 12 => 31,
        4, 6, 9, 11 => 30,
        2 => if (isLeapYear(year)) 29 else 28,
        else => 0,
    };
}

fn isLeapYear(year: u16) bool {
    return (year % 4 == 0 and year % 100 != 0) or (year % 400 == 0);
}
