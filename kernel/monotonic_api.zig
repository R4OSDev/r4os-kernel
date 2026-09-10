// One ABI mapping for R4SYS and the optional R4D clock entry.
// Read-only: no driver lifecycle guard, allocation, wait or owner mutation.
const a = @import("r4os_kernel_contract");
const time_core = @import("../platform/time.zig");
const MonotonicClockInfo = a.MonotonicClockInfo;

pub fn monotonicClock(out: *MonotonicClockInfo) callconv(.c) i32 {
    comptime {
        if (time_core.monotonic_flag_valid != a.monotonic_clock_flag_valid or
            time_core.monotonic_flag_continuous != a.monotonic_clock_flag_continuous or
            time_core.monotonic_flag_high_resolution != a.monotonic_clock_flag_high_resolution or
            time_core.monotonic_flag_irq_independent != a.monotonic_clock_flag_irq_independent or
            time_core.monotonic_flag_invariant != a.monotonic_clock_flag_invariant or
            time_core.monotonic_flag_early_origin != a.monotonic_clock_flag_early_origin or
            time_core.monotonic_flag_calibrated != a.monotonic_clock_flag_calibrated or
            time_core.monotonic_flag_degraded != a.monotonic_clock_flag_degraded)
        {
            @compileError("monotonic clock flag contract drift");
        }
    }
    const clock = time_core.monotonicSnapshot();
    out.* = .{
        .flags = clock.flags,
        .source = @intFromEnum(clock.source),
        .generation = clock.generation,
        .event_backend = switch (clock.event_backend) {
            .pit => 0,
            .hpet => 1,
            .lapic => 2,
        },
        .instant_ns = clock.instant_ns,
        .frequency_hz = a.monotonic_clock_frequency_hz,
        .resolution_ns = clock.resolution_ns,
        .source_frequency_hz = clock.source_frequency_hz,
        .event_frequency_numerator = clock.event.frequency_numerator,
        .event_frequency_denominator = clock.event.frequency_denominator,
        .event_requested_hz = clock.event.requested_hz,
        .event_effective_hz = clock.event.effective_hz,
    };
    return if (clock.valid) 1 else 0;
}
