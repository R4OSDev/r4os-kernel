const interrupts = @import("../../arch/x86_64/interrupts.zig");
const hpet = @import("../../arch/x86_64/hpet.zig");
const timer = @import("../../kernel/timer.zig");
const platform_cpu = @import("../../platform/cpu.zig");
const scheduler = @import("../../sched/scheduler.zig");
const timing = @import("usb_boot_timing.zig");
const std = @import("std");

const MIN_PLAUSIBLE_TSC_HZ: u64 = 1_000_000;
const MAX_PLAUSIBLE_TSC_HZ: u64 = 10_000_000_000;
// A deliberately high fallback can only make a deadline later on realistic
// x86_64 hardware; it must never turn a requested two seconds into 70 ms.
const FALLBACK_TSC_HZ: u64 = 10_000_000_000;
// Last-resort bounded delay for a machine exposing neither an advancing IRQ
// clock nor HPET/TSC. It is deliberately only a liveness guard, not a claimed
// wall-clock calibration.
const NO_CLOCK_SPINS_PER_MS: u64 = 100_000;

pub const Deadline = struct {
    flags: u64,
    start_tick: u64,
    duration_ticks: u64,
    start_hpet: u64,
    duration_hpet: u64,
    hpet_mask: u64,
    start_tsc: u64,
    duration_tsc: u64,
    fallback_tsc: bool,

    pub fn begin(milliseconds_value: u32) Deadline {
        const flags = interrupts.saveAndDisable();
        const hpet_info = hpet.status();
        const hpet_mask: u64 = if (hpet_info.counter_64bit)
            std.math.maxInt(u64)
        else
            std.math.maxInt(u32);
        const duration_hpet = if (hpet_info.mapped and
            hpet_info.enabled and
            hpet_info.frequency_hz != 0)
            clockCyclesForMilliseconds(
                hpet_info.frequency_hz,
                milliseconds_value,
                hpet_mask,
            )
        else
            0;
        // Early enumeration has no task yet and deliberately borrows IRQs so
        // PIT ticks can form a wall-clock deadline.  A runtime caller that
        // entered with IF=0 is inside a critical section: never silently
        // enable interrupts there. The free-running HPET remains usable with
        // IF=0; an invariant TSC is the second independent clock.
        const may_enable = interrupts.wereEnabled(flags) or scheduler.current() == null;
        const duration_ticks = if (may_enable)
            timing.ticksForMilliseconds(milliseconds_value, timer.frequency())
        else
            0;
        const cpu_info = platform_cpu.status();
        const trusted_tsc = cpu_info.features.tsc and cpu_info.features.invariant_tsc;
        const exact_tsc_hz = if (trusted_tsc) exactTscFrequencyHz(cpu_info) else 0;
        const has_wall_clock = duration_ticks != 0 or duration_hpet != 0;
        // Very old hardware can expose neither HPET nor an invariant TSC.
        // Keep an ordinary TSC only as a finite last-resort escape hatch when
        // no wall clock can advance. An invariant TSC may run in parallel,
        // but only CPUID.15 supplies an exact rate; otherwise the conservative
        // 10-GHz bound can delay, never prematurely shorten, the deadline.
        const use_tsc = cpu_info.features.tsc and (trusted_tsc or !has_wall_clock);
        const fallback_tsc = use_tsc and exact_tsc_hz == 0;
        const duration_tsc = if (use_tsc)
            clockCyclesForMilliseconds(
                if (exact_tsc_hz != 0) exact_tsc_hz else FALLBACK_TSC_HZ,
                milliseconds_value,
                std.math.maxInt(u64),
            )
        else
            0;
        const out = Deadline{
            .flags = flags,
            .start_tick = timer.tickCount(),
            .duration_ticks = duration_ticks,
            .start_hpet = if (duration_hpet != 0) hpet.readMainCounter() & hpet_mask else 0,
            .duration_hpet = duration_hpet,
            .hpet_mask = hpet_mask,
            .start_tsc = if (duration_tsc != 0) readTsc() else 0,
            .duration_tsc = duration_tsc,
            .fallback_tsc = fallback_tsc,
        };
        if (may_enable) interrupts.enable();
        return out;
    }

    pub fn expired(self: *const Deadline) bool {
        return self.duration_ticks != 0 and timer.tickCount() -% self.start_tick >= self.duration_ticks;
    }

    pub fn usesTickDeadline(self: *const Deadline) bool {
        return self.duration_ticks != 0;
    }

    pub fn hpetExpired(self: *const Deadline) bool {
        return self.duration_hpet != 0 and self.elapsedHpet() >= self.duration_hpet;
    }

    pub fn usesHpetDeadline(self: *const Deadline) bool {
        return self.duration_hpet != 0;
    }

    pub fn tscExpired(self: *const Deadline) bool {
        return self.duration_tsc != 0 and self.elapsedTsc() >= self.duration_tsc;
    }

    pub fn usesTscDeadline(self: *const Deadline) bool {
        return self.duration_tsc != 0;
    }

    pub fn usesFallbackTsc(self: *const Deadline) bool {
        return self.fallback_tsc;
    }

    pub fn hasClock(self: *const Deadline) bool {
        return self.duration_ticks != 0 or
            self.duration_hpet != 0 or
            self.duration_tsc != 0;
    }

    pub fn expiredAny(self: *const Deadline) bool {
        return self.expired() or self.hpetExpired() or self.tscExpired();
    }

    pub fn elapsedTicks(self: *const Deadline) u64 {
        return timer.tickCount() -% self.start_tick;
    }

    pub fn elapsedHpet(self: *const Deadline) u64 {
        if (self.duration_hpet == 0) return 0;
        const now = hpet.readMainCounter() & self.hpet_mask;
        return (now -% self.start_hpet) & self.hpet_mask;
    }

    pub fn elapsedTsc(self: *const Deadline) u64 {
        if (self.duration_tsc == 0) return 0;
        return readTsc() -% self.start_tsc;
    }

    pub fn finish(self: *const Deadline) void {
        interrupts.restore(self.flags);
    }
};

// Storage discovery runs before the scheduler. Temporarily enable IRQs so the
// early PIT can provide a real time base, then restore the caller's IF state.
pub fn milliseconds(value: u32) void {
    if (value == 0) return;
    var deadline = Deadline.begin(value);
    defer deadline.finish();
    if (!deadline.hasClock()) {
        var guard: u64 = @as(u64, value) * NO_CLOCK_SPINS_PER_MS;
        while (guard != 0) : (guard -= 1) {
            asm volatile ("pause");
        }
        return;
    }
    while (!deadline.expiredAny()) {
        asm volatile ("pause");
    }
}

fn clockCyclesForMilliseconds(
    frequency: u64,
    milliseconds_value: u32,
    maximum: u64,
) u64 {
    if (frequency == 0 or milliseconds_value == 0) return 0;
    const product = @as(u128, frequency) * milliseconds_value;
    const cycles = (product + 999) / 1000;
    const maximum_wide = @as(u128, maximum);
    // Modular elapsed-time comparison needs the requested interval to fit in
    // one hardware-counter revolution. Disable this source otherwise.
    if (cycles == 0 or cycles > maximum_wide) return 0;
    return @intCast(cycles);
}

fn exactTscFrequencyHz(info: platform_cpu.Status) u64 {
    if (info.tsc_denominator != 0 and info.tsc_numerator != 0 and info.crystal_hz != 0) {
        const frequency = (@as(u128, info.crystal_hz) * info.tsc_numerator) / info.tsc_denominator;
        if (frequency >= MIN_PLAUSIBLE_TSC_HZ and frequency <= MAX_PLAUSIBLE_TSC_HZ)
            return @intCast(frequency);
    }
    // CPUID.16 base_mhz describes the processor base frequency, not a
    // guaranteed TSC frequency. Treating it as exact can make a parallel TSC
    // source beat a valid PIT/HPET deadline far too early.
    return 0;
}

fn readTsc() u64 {
    var lo: u32 = undefined;
    var hi: u32 = undefined;
    // Order preceding MMIO/event-ring observations before the timestamp so a
    // speculative RDTSC cannot shorten the visible timeout window.
    asm volatile ("lfence");
    asm volatile ("rdtsc"
        : [lo] "={eax}" (lo),
          [hi] "={edx}" (hi),
    );
    return (@as(u64, hi) << 32) | lo;
}
