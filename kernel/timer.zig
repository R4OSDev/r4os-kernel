// Private C projection. Event epochs and BSP deadline state have one Rust owner.
const pit = @import("../arch/x86_64/pit.zig");
comptime {
    if (@import("builtin").is_test) {
        // A host unit can use only a scalar timer helper while its native C
        // dispatcher still links the actual hardware-owner branches. Admit
        // their canonical bridges in this same module, without duplicate
        // owners, fake clocks or guest hardware execution in the fixture.
        _ = @import("rust_owner_test");
        _ = @import("../arch/x86_64/hpet.zig");
        _ = @import("../arch/x86_64/ioapic.zig");
        _ = @import("../arch/x86_64/lapic.zig");
        _ = @import("../arch/x86_64/interrupts.zig");
        _ = @import("../memory/owner_locks.zig");
        _ = @import("../platform/clocksource.zig");
    }
}
pub const Backend = enum {
    pit,
    hpet,
    lapic,
};

pub const PIT_IRQ = pit.IRQ;
pub const DEFAULT_HZ = pit.DEFAULT_HZ;
pub const NO_DEADLINE: u64 = ~@as(u64, 0);
pub const MAX_FINITE_DEADLINE: u64 = NO_DEADLINE - 1;

pub const Mode = enum {
    periodic,
    one_shot_idle,
};

pub const EventClockInfo = extern struct {
    requested_hz: u32 = 0,
    effective_hz: u32 = 0,
    frequency_numerator: u64 = 0,
    frequency_denominator: u64 = 0,
    resolution_ns: u64 = 0,
};

pub const DeadlineStats = struct {
    enabled: bool = false,
    capable: bool = false,
    mode: Mode = .periodic,
    armed_deadline: u64 = NO_DEADLINE,
    timer_irqs: u64 = 0,
    idle_entries: u64 = 0,
    one_shot_arms: u64 = 0,
    earlier_reprograms: u64 = 0,
    cancels: u64 = 0,
    periodic_resumes: u64 = 0,
    runtime_fallbacks: u64 = 0,
    late_irqs: u64 = 0,
    max_lateness_ticks: u64 = 0,
    last_lateness_ticks: u64 = 0,
    last_irq_tick: u64 = 0,
};

const RawDeadlineStats = extern struct {
    flags: u32,
    mode: u32,
    armed_deadline: u64,
    timer_irqs: u64,
    idle_entries: u64,
    one_shot_arms: u64,
    earlier_reprograms: u64,
    cancels: u64,
    periodic_resumes: u64,
    runtime_fallbacks: u64,
    late_irqs: u64,
    max_lateness_ticks: u64,
    last_lateness_ticks: u64,
    last_irq_tick: u64,
};
extern fn r4os_rust_timer_control(operation: u8, a: u64, b: u64) u64;
extern fn r4os_rust_timer_event_info(output: *EventClockInfo) void;
extern fn r4os_rust_timer_deadlines(output: *RawDeadlineStats) void;
export fn r4os_zig_timer_serial_drain() callconv(.c) void {
    if (comptime @import("builtin").is_test) @panic("Timer serial drain requires the kernel guest");
    @import("../driver/com.zig").logDrain();
}
pub fn initPit(hz: u32) void {
    _ = r4os_rust_timer_control(0, hz, 0);
}
pub fn trySwitchToLapic(hz: u32) bool {
    return r4os_rust_timer_control(1, hz, 0) != 0;
}
pub fn trySwitchToHpet(hz: u32) bool {
    return r4os_rust_timer_control(2, hz, 0) != 0;
}
pub fn fallbackToPit() void {
    _ = r4os_rust_timer_control(3, 0, 0);
}
pub fn onIrq() u64 {
    return r4os_rust_timer_control(4, 0, 0);
}
pub fn tickCount() u64 {
    return r4os_rust_timer_control(5, 0, 0);
}
pub fn deadlineAfter(now: u64, ticks: u64) u64 {
    return r4os_rust_timer_control(6, now, ticks);
}
pub fn deadlineAfterNow(ticks: u64) u64 {
    return r4os_rust_timer_control(7, ticks, 0);
}
pub fn remainingUntil(now: u64, deadline: u64) u64 {
    return r4os_rust_timer_control(8, now, deadline);
}
pub fn eventNanoseconds() u64 {
    return r4os_rust_timer_control(9, 0, 0);
}
pub fn frequency() u32 {
    return @intCast(r4os_rust_timer_control(10, 0, 0));
}
pub fn enableDeadlineScheduling() bool {
    return r4os_rust_timer_control(11, 0, 0) != 0;
}
pub fn enterIdleDeadline(deadline: u64) bool {
    return r4os_rust_timer_control(12, deadline, 0) != 0;
}
pub fn leaveIdleDeadline() bool {
    return r4os_rust_timer_control(13, 0, 0) != 0;
}
pub fn activeBackend() Backend {
    return @enumFromInt(r4os_rust_timer_control(14, 0, 0));
}
pub fn deadlineModeName() []const u8 {
    return switch (r4os_rust_timer_control(15, 0, 0)) {
        0 => "periodic-fallback",
        1 => "periodic-active",
        2 => "one-shot-idle",
        else => unreachable,
    };
}
pub fn backendName() []const u8 {
    return switch (activeBackend()) {
        .pit => "PIT",
        .hpet => "HPET",
        .lapic => "LAPIC timer",
    };
}
pub fn eventClockInfo() EventClockInfo {
    var value: EventClockInfo = undefined;
    r4os_rust_timer_event_info(&value);
    return value;
}
pub fn deadlineStats() DeadlineStats {
    var raw: RawDeadlineStats = undefined;
    r4os_rust_timer_deadlines(&raw);
    return .{ .enabled = raw.flags & 1 != 0, .capable = raw.flags & 2 != 0, .mode = @enumFromInt(raw.mode), .armed_deadline = raw.armed_deadline, .timer_irqs = raw.timer_irqs, .idle_entries = raw.idle_entries, .one_shot_arms = raw.one_shot_arms, .earlier_reprograms = raw.earlier_reprograms, .cancels = raw.cancels, .periodic_resumes = raw.periodic_resumes, .runtime_fallbacks = raw.runtime_fallbacks, .late_irqs = raw.late_irqs, .max_lateness_ticks = raw.max_lateness_ticks, .last_lateness_ticks = raw.last_lateness_ticks, .last_irq_tick = raw.last_irq_tick };
}
comptime {
    if (PIT_IRQ != 0 or DEFAULT_HZ != 1000 or @sizeOf(EventClockInfo) != 32 or @offsetOf(EventClockInfo, "frequency_numerator") != 8 or
        @sizeOf(RawDeadlineStats) != 104 or @offsetOf(RawDeadlineStats, "last_irq_tick") != 96)
        @compileError("Rust timer C layout or platform constants differ");
}
