//! Compile-time boundary for optional diagnostic work. Functional clocks,
//! deadlines, owner tracking and resource accounting never use this module.
const config = @import("config");
const monotonic = @import("../platform/monotonic.zig");

pub const detailed = config.enable_metrics;
pub const work_critical_stride: u32 = 64;

pub fn capture() monotonic.Stamp {
    if (comptime !detailed) return .{};
    return monotonic.capture();
}
