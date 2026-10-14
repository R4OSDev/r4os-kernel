// One ABI mapping for R4SYS and the optional R4D clock entry.
// Read-only: no driver lifecycle guard, allocation, wait or owner mutation.
const a = @import("r4os_kernel_contract");
const time_core = @import("../platform/time.zig");
const MonotonicClockInfo = a.MonotonicClockInfo;
extern fn r4os_rust_monotonic_clock_api(output: *MonotonicClockInfo) i32;

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
    return r4os_rust_monotonic_clock_api(out);
}
