const r4x_api = @import("../program/r4x_api.zig");
const rtc = @import("../arch/x86_64/rtc.zig");
const acpi = @import("acpi.zig");
const bootlog = @import("../kernel/bootlog.zig");
const timer = @import("../kernel/timer.zig");
const monotonic = @import("monotonic.zig");

pub const WallClock = rtc.DateTime;

pub const State = r4x_api.TimeState;
pub const MonotonicSnapshot = monotonic.Snapshot;
pub const MonotonicStamp = monotonic.Stamp;

pub const monotonic_flag_valid = monotonic.flag_valid;
pub const monotonic_flag_continuous = monotonic.flag_continuous;
pub const monotonic_flag_high_resolution = monotonic.flag_high_resolution;
pub const monotonic_flag_irq_independent = monotonic.flag_irq_independent;
pub const monotonic_flag_invariant = monotonic.flag_invariant;
pub const monotonic_flag_early_origin = monotonic.flag_early_origin;
pub const monotonic_flag_calibrated = monotonic.flag_calibrated;
pub const monotonic_flag_degraded = monotonic.flag_degraded;

pub fn wallClock() WallClock {
    return rtc.readDateTime(acpi.info().fadt_century);
}

extern fn r4os_rust_time_state(output: *State) void;
extern fn r4os_rust_time_set_state(input: *const State) i32;
pub fn state() State {
    var output: State = undefined;
    r4os_rust_time_state(&output);
    return output;
}
pub fn setState(next: State) i32 {
    return r4os_rust_time_set_state(&next);
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

pub fn attachHpetClock() void {
    monotonic.attachHpetClock();
}

pub fn monotonicSnapshot() MonotonicSnapshot {
    return monotonic.snapshot();
}

pub fn monotonicNanoseconds() ?u64 {
    return monotonic.nowNanoseconds();
}

pub fn monotonicCapture() MonotonicStamp {
    return monotonic.capture();
}

pub fn monotonicResolve(stamp: MonotonicStamp) ?u64 {
    return monotonic.resolve(stamp);
}

pub fn monotonicElapsed(start: MonotonicStamp, end: MonotonicStamp) ?u64 {
    return monotonic.elapsedNanoseconds(start, end);
}

pub fn monotonicElapsedSince(start: MonotonicStamp) ?u64 {
    return monotonic.elapsedSince(start);
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
    const clock = monotonic.snapshot();
    bootlog.puts(switch (clock.source) {
        .unavailable => "unavailable",
        .tsc => "TSC",
        .hpet => "HPET main counter",
        .periodic_event => timer.backendName(),
    });
    bootlog.puts(" event=");
    bootlog.puts(timer.backendName());
    const hardware = monotonic.hardwareStatus();
    bootlog.puts(" source_hz=");
    bootlog.putDec(hardware.frequency_hz);
    bootlog.puts(" tsc_invariant=");
    bootlog.puts(if (hardware.tsc_invariant) "yes" else "no");
    bootlog.puts(" hpet_calibrated=");
    bootlog.puts(if (hardware.tsc_hpet_calibrated) "yes" else "no");
    bootlog.puts(" calibration_ppm=");
    bootlog.putDec(hardware.calibration_error_ppm);
    bootlog.puts(" fallback=");
    bootlog.puts(monotonic.hardwareFallbackReasonName(hardware.fallback_reason));
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

fn bootlogPut2(value: u64) void {
    if (value < 10) bootlog.puts("0");
    bootlog.putDec(value);
}
