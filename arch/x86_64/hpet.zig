const acpi = @import("../../platform/acpi.zig");
const bootlog = @import("../../kernel/bootlog.zig");
const k = @import("../../kernel/log.zig");
extern fn r4os_rust_hpet_control(operation: u8, a: u64, b: u32) u64;
extern fn r4os_rust_hpet_elapsed(start: u64, end: u64) u64;
extern fn r4os_rust_hpet_init(base: u64, number: u8, min_tick: u16, legacy: bool, output: *RawStatus) void;
extern fn r4os_rust_hpet_status(output: *RawStatus) void;
pub const RawStatus = extern struct {
    flags: u32,
    comparator_count: u8,
    revision: u8,
    number: u8,
    reason: u8,
    min_tick: u16,
    vendor_id: u16,
    period_fs: u32,
    frequency_hz: u64,
    phys_base: u64,
    virt_base: u64,
    config: u64,
    timer0_config: u64,
    timer0_comparator: u64,
    timer0_period: u64,
    counter: u64,
    timer_ticks: u64,
    timer_hz: u32,
};
pub const Status = struct {
    available: bool = false,
    mapped: bool = false,
    enabled: bool = false,
    legacy_replacement_capable: bool = false,
    counter_64bit: bool = false,
    timer0_periodic_capable: bool = false,
    timer0_64bit_capable: bool = false,
    timer0_irq_active: bool = false,
    timer0_one_shot: bool = false,
    comparator_count: u8 = 0,
    revision: u8 = 0,
    number: u8 = 0,
    min_tick: u16 = 0,
    period_fs: u32 = 0,
    frequency_hz: u64 = 0,
    vendor_id: u16 = 0,
    phys_base: u64 = 0,
    virt_base: u64 = 0,
    config: u64 = 0,
    timer0_config: u64 = 0,
    timer0_comparator: u64 = 0,
    timer0_period: u64 = 0,
    counter: u64 = 0,
    timer_ticks: u64 = 0,
    timer_hz: u32 = 0,
    reason: []const u8 = "not initialized",
};

pub fn project(raw: RawStatus) Status {
    return .{ .available = raw.flags & 1 != 0, .mapped = raw.flags & 2 != 0, .enabled = raw.flags & 4 != 0, .legacy_replacement_capable = raw.flags & 8 != 0, .counter_64bit = raw.flags & 16 != 0, .timer0_periodic_capable = raw.flags & 32 != 0, .timer0_64bit_capable = raw.flags & 64 != 0, .timer0_irq_active = raw.flags & 128 != 0, .timer0_one_shot = raw.flags & 256 != 0, .comparator_count = raw.comparator_count, .revision = raw.revision, .number = raw.number, .min_tick = raw.min_tick, .vendor_id = raw.vendor_id, .period_fs = raw.period_fs, .frequency_hz = raw.frequency_hz, .phys_base = raw.phys_base, .virt_base = raw.virt_base, .config = raw.config, .timer0_config = raw.timer0_config, .timer0_comparator = raw.timer0_comparator, .timer0_period = raw.timer0_period, .counter = raw.counter, .timer_ticks = raw.timer_ticks, .timer_hz = raw.timer_hz, .reason = switch (raw.reason) {
        0 => "not initialized",
        1 => "ACPI HPET missing",
        2 => "HPET MMIO mapping failed",
        3 => "HPET main counter did not advance within bounded qualification",
        4 => "MMIO mapped, main counter enabled, interrupts disabled",
        5 => "HPET timer unavailable",
        6 => "HPET legacy replacement not supported",
        7 => "HPET comparator 0 is not periodic capable",
        8 => "HPET comparator 0 periodic legacy IRQ0 active",
        9 => "HPET comparator 0 setup incomplete",
        10 => "HPET one-shot unavailable",
        11 => "HPET one-shot needs legacy IRQ0 replacement",
        12 => "HPET comparator 0 one-shot legacy IRQ0 ready",
        13 => "HPET one-shot setup incomplete",
        14 => "HPET comparator 0 one-shot armed",
        15 => "HPET comparator 0 one-shot disarmed",
        16 => "HPET main counter enabled, comparator interrupts disabled",
        else => unreachable,
    } };
}
pub fn initFromAcpi(info: acpi.Info) Status {
    var raw: RawStatus = undefined;
    r4os_rust_hpet_init(info.hpet_base, info.hpet_number, info.hpet_min_tick, info.hpet_legacy_replacement, &raw);
    return project(raw);
}
pub fn status() Status {
    var raw: RawStatus = undefined;
    r4os_rust_hpet_status(&raw);
    return project(raw);
}
pub fn readMainCounter() u64 {
    return r4os_rust_hpet_control(0, 0, 0);
}
pub fn readExtendedMainCounter() u64 {
    return r4os_rust_hpet_control(1, 0, 0);
}
pub fn elapsedMainCounter(start: u64, end: u64) u64 {
    return r4os_rust_hpet_elapsed(start, end);
}
pub fn disableUnusableCounter() void {
    _ = r4os_rust_hpet_control(12, 0, 0);
}
pub fn startLegacyIrqTimer(hz: u32) bool {
    return r4os_rust_hpet_control(6, 0, hz) != 0;
}
pub fn resumeLegacyIrqTimer(hz: u32) bool {
    return r4os_rust_hpet_control(7, 0, hz) != 0;
}
pub fn startLegacyOneShotTimer(hz: u32) bool {
    return r4os_rust_hpet_control(8, 0, hz) != 0;
}
pub fn armLegacyOneShotTicks(ticks: u64, hz: u32) u64 {
    return r4os_rust_hpet_control(9, ticks, hz);
}
pub fn disarmLegacyOneShotTimer() void {
    _ = r4os_rust_hpet_control(10, 0, 0);
}
pub fn stopLegacyIrqTimer() void {
    _ = r4os_rust_hpet_control(11, 0, 0);
}
pub fn onTimerIrq() u64 {
    return r4os_rust_hpet_control(3, 0, 0);
}
pub fn timerTickCount() u64 {
    return r4os_rust_hpet_control(4, 0, 0);
}
pub fn timerFrequency() u32 {
    return @intCast(r4os_rust_hpet_control(5, 0, 0));
}
comptime {
    if (@sizeOf(RawStatus) != 96 or @offsetOf(RawStatus, "timer_hz") != 88) @compileError("Rust HPET C layout differs");
}

pub fn dumpStatus() void {
    const s = status();
    k.puts("HPET status\r\n");
    k.puts("  Available: ");
    k.puts(if (s.available) "yes" else "no");
    k.puts(" mapped=");
    k.puts(if (s.mapped) "yes" else "no");
    k.puts(" enabled=");
    k.puts(if (s.enabled) "yes" else "no");
    k.puts("\r\n");
    k.puts("  Base: phys=0x");
    k.putHex(s.phys_base, 16);
    k.puts(" virt=0x");
    k.putHex(s.virt_base, 16);
    k.puts("\r\n");
    k.puts("  Caps: rev=");
    k.putDec(s.revision);
    k.puts(" timers=");
    k.putDec(s.comparator_count);
    k.puts(" 64bit=");
    k.puts(if (s.counter_64bit) "yes" else "no");
    k.puts(" legacy=");
    k.puts(if (s.legacy_replacement_capable) "yes" else "no");
    k.puts(" vendor=0x");
    k.putHex(s.vendor_id, 4);
    k.puts("\r\n");
    k.puts("  Clock: period_fs=");
    k.putDec(s.period_fs);
    k.puts(" freq_hz=");
    k.putDec(s.frequency_hz);
    k.puts(" min_tick=");
    k.putDec(s.min_tick);
    k.puts(" number=");
    k.putDec(s.number);
    k.puts("\r\n");
    k.puts("  Config: 0x");
    k.putHex(s.config, 16);
    k.puts(" counter=");
    k.putDec(s.counter);
    k.puts("\r\n");
    k.puts("  Timer0: periodic_cap=");
    k.puts(if (s.timer0_periodic_capable) "yes" else "no");
    k.puts(" 64bit_cap=");
    k.puts(if (s.timer0_64bit_capable) "yes" else "no");
    k.puts(" irq_active=");
    k.puts(if (s.timer0_irq_active) "yes" else "no");
    k.puts(" one_shot=");
    k.puts(if (s.timer0_one_shot) "yes" else "no");
    k.puts(" hz=");
    k.putDec(s.timer_hz);
    k.puts(" ticks=");
    k.putDec(s.timer_ticks);
    k.puts(" period=");
    k.putDec(s.timer0_period);
    k.puts("\r\n");
    k.puts("  Timer0 config=0x");
    k.putHex(s.timer0_config, 16);
    k.puts(" comparator=");
    k.putDec(s.timer0_comparator);
    k.puts("\r\n");
    k.puts("  Note: ");
    k.puts(s.reason);
    k.puts("\r\n");
}

export fn r4os_zig_hpet_log(raw: *const RawStatus) callconv(.c) void {
    if (comptime @import("builtin").is_test) return;
    const s = project(raw.*);
    bootlog.puts("[HPET] ");
    bootlog.puts(if (s.available) "available" else "missing");
    bootlog.puts(" mapped=");
    bootlog.puts(if (s.mapped) "yes" else "no");
    bootlog.puts(" enabled=");
    bootlog.puts(if (s.enabled) "yes" else "no");
    bootlog.puts(" base=0x");
    bootlog.putHex(s.phys_base, 16);
    bootlog.puts(" period_fs=");
    bootlog.putDec(s.period_fs);
    bootlog.puts(" freq_hz=");
    bootlog.putDec(s.frequency_hz);
    bootlog.puts(" counter=");
    bootlog.putDec(s.counter);
    bootlog.puts(" timer0_irq=");
    bootlog.puts(if (s.timer0_irq_active) "on" else "off");
    bootlog.puts(" timer_hz=");
    bootlog.putDec(s.timer_hz);
    bootlog.puts(" timer_ticks=");
    bootlog.putDec(s.timer_ticks);
    bootlog.puts(" reason=");
    bootlog.puts(s.reason);
    bootlog.puts("\r\n");
}
