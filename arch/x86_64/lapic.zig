// Private fixed C facade. CPU-local APIC operations and BSP timer state live in Rust.
const acpi = @import("../../platform/acpi.zig");
const bootlog = @import("../../kernel/bootlog.zig");
const k = @import("../../kernel/log.zig");
const platform_cpu = @import("../../platform/cpu.zig");
pub const SPURIOUS_VECTOR: u8 = 0xFF;
const TIMER_VECTOR: u8 = 0x20;
const TIMER_DIVIDE_BY_16: u32 = 0x3;
pub const RawStatus = extern struct {
    phys_base: u64,
    msr_base: u64,
    virt_base: u64,
    timer_ticks: u64,
    calibration_hpet_ticks: u64,
    flags: u32,
    svr: u32,
    timer_frequency_hz: u32,
    timer_initial_count: u32,
    timer_current_count: u32,
    timer_divide: u32,
    timer_lvt: u32,
    calibration_lapic_ticks: u32,
    id: u8,
    version: u8,
    max_lvt: u8,
    timer_vector: u8,
    reason: u8,
    timer_reason: u8,
    reserved: [2]u8,
};
pub const Status = struct {
    available: bool = false,
    enabled: bool = false,
    software_enabled: bool = false,
    timer_enabled: bool = false,
    timer_calibrated: bool = false,
    timer_one_shot: bool = false,
    bsp: bool = false,
    x2apic: bool = false,
    phys_base: u64 = 0,
    msr_base: u64 = 0,
    virt_base: u64 = 0,
    id: u8 = 0,
    version: u8 = 0,
    max_lvt: u8 = 0,
    svr: u32 = 0,
    timer_vector: u8 = TIMER_VECTOR,
    timer_frequency_hz: u32 = 0,
    timer_initial_count: u32 = 0,
    timer_current_count: u32 = 0,
    timer_ticks: u64 = 0,
    timer_divide: u32 = TIMER_DIVIDE_BY_16,
    timer_lvt: u32 = 0,
    calibration_hpet_ticks: u64 = 0,
    calibration_lapic_ticks: u32 = 0,
    reason: []const u8 = "not initialized",
    timer_reason: []const u8 = "timer not initialized",
};

extern fn r4os_rust_lapic_init(address: u64, enabled_count: u32, supports_x2: bool, output: *RawStatus) void;
extern fn r4os_rust_lapic_status(output: *RawStatus) void;
extern fn r4os_rust_lapic_control(operation: u8, a: u64, b: u32) u64;
pub fn project(raw: RawStatus) Status {
    return .{ .available = raw.flags & 1 != 0, .enabled = raw.flags & 2 != 0, .software_enabled = raw.flags & 4 != 0, .timer_enabled = raw.flags & 8 != 0, .timer_calibrated = raw.flags & 16 != 0, .timer_one_shot = raw.flags & 32 != 0, .bsp = raw.flags & 64 != 0, .x2apic = raw.flags & 128 != 0, .phys_base = raw.phys_base, .msr_base = raw.msr_base, .virt_base = raw.virt_base, .id = raw.id, .version = raw.version, .max_lvt = raw.max_lvt, .svr = raw.svr, .timer_vector = raw.timer_vector, .timer_frequency_hz = raw.timer_frequency_hz, .timer_initial_count = raw.timer_initial_count, .timer_current_count = raw.timer_current_count, .timer_ticks = raw.timer_ticks, .timer_divide = raw.timer_divide, .timer_lvt = raw.timer_lvt, .calibration_hpet_ticks = raw.calibration_hpet_ticks, .calibration_lapic_ticks = raw.calibration_lapic_ticks, .reason = switch (raw.reason) {
        0 => "not initialized",
        1 => "MADT has no enabled LAPIC",
        2 => "x2APIC MSR mode active, PIC/IOAPIC routing unchanged",
        3 => "LAPIC base missing",
        4 => "LAPIC MMIO mapping failed",
        5 => "xAPIC MMIO enabled, PIC/IOAPIC routing unchanged",
        else => unreachable,
    }, .timer_reason = switch (raw.timer_reason) {
        0 => "timer not initialized",
        1 => "LAPIC not enabled",
        2 => "HPET reference clock unavailable",
        3 => "HPET reference too slow",
        4 => "LAPIC timer did not count during calibration",
        5 => "LAPIC calibration denominator is zero",
        6 => "LAPIC calibrated count out of range",
        7 => "calibrated against HPET",
        8 => "periodic LAPIC timer active",
        9 => "LAPIC one-shot unavailable without calibration",
        10 => "LAPIC one-shot ready",
        11 => "LAPIC one-shot armed",
        12 => "LAPIC one-shot disarmed",
        13 => "LAPIC periodic resume unavailable",
        else => unreachable,
    } };
}
pub fn initFromAcpi(info: acpi.Info) Status {
    var raw: RawStatus = undefined;
    r4os_rust_lapic_init(info.madt_lapic_address, info.madt_lapic_enabled_count, platform_cpu.status().features.x2apic, &raw);
    return project(raw);
}
pub fn status() Status {
    var raw: RawStatus = undefined;
    r4os_rust_lapic_status(&raw);
    return project(raw);
}
pub fn isEnabled() bool {
    return r4os_rust_lapic_control(0, 0, 0) != 0;
}
pub fn initCurrentCpu() bool {
    return r4os_rust_lapic_control(1, 0, 0) != 0;
}
pub fn localApicId() u32 {
    return @intCast(r4os_rust_lapic_control(2, 0, 0));
}
pub fn startSecondaryPeriodicTimer(hz: u32) bool {
    return r4os_rust_lapic_control(3, 0, hz) != 0;
}
pub fn sendInitSipi(apic_id: u32, vector: u8) bool {
    return r4os_rust_lapic_control(4, apic_id, vector) != 0;
}
pub fn sendReschedule(apic_id: u32, vector: u8) bool {
    return r4os_rust_lapic_control(5, apic_id, vector) != 0;
}
pub fn sendStop(apic_id: u32, vector: u8) bool {
    return r4os_rust_lapic_control(6, apic_id, vector) != 0;
}
pub fn sendIpi(apic_id: u32, vector: u8) bool {
    return r4os_rust_lapic_control(7, apic_id, vector) != 0;
}
pub fn endOfInterrupt() void {
    _ = r4os_rust_lapic_control(8, 0, 0);
}
pub fn onTimerIrq() u64 {
    return r4os_rust_lapic_control(9, 0, 0);
}
pub fn timerTickCount() u64 {
    return r4os_rust_lapic_control(10, 0, 0);
}
pub fn timerFrequency() u32 {
    return @intCast(r4os_rust_lapic_control(11, 0, 0));
}
pub fn initTimerFromHpet(hz: u32) bool {
    return r4os_rust_lapic_control(12, 0, hz) != 0;
}
pub fn startOneShotTimer() bool {
    return r4os_rust_lapic_control(13, 0, 0) != 0;
}
pub fn armOneShotTicks(ticks: u64) u64 {
    return r4os_rust_lapic_control(14, ticks, 0);
}
pub fn disarmOneShotTimer() void {
    _ = r4os_rust_lapic_control(15, 0, 0);
}
pub fn resumePeriodicTimer() bool {
    return r4os_rust_lapic_control(16, 0, 0) != 0;
}
pub fn stopTimer() void {
    _ = r4os_rust_lapic_control(17, 0, 0);
}
pub fn isTimerActive() bool {
    return r4os_rust_lapic_control(18, 0, 0) != 0;
}
comptime {
    if (@sizeOf(RawStatus) != 80 or @offsetOf(RawStatus, "flags") != 40 or @offsetOf(RawStatus, "timer_reason") != 77)
        @compileError("Rust LAPIC C layout differs");
}

pub fn dumpStatus() void {
    const s = status();
    k.puts("LAPIC status\r\n");
    k.puts("  Available: ");
    k.puts(if (s.available) "yes" else "no");
    k.puts(" enabled=");
    k.puts(if (s.enabled) "yes" else "no");
    k.puts(" software=");
    k.puts(if (s.software_enabled) "yes" else "no");
    k.puts(" bsp=");
    k.puts(if (s.bsp) "yes" else "no");
    k.puts(" x2apic=");
    k.puts(if (s.x2apic) "yes" else "no");
    k.puts("\r\n");
    k.puts("  Base: madt=0x");
    k.putHex(s.phys_base, 16);
    k.puts(" msr=0x");
    k.putHex(s.msr_base, 16);
    k.puts(" virt=0x");
    k.putHex(s.virt_base, 16);
    k.puts("\r\n");
    k.puts("  ID=");
    k.putDec(s.id);
    k.puts(" version=0x");
    k.putHex(s.version, 2);
    k.puts(" max_lvt=");
    k.putDec(s.max_lvt);
    k.puts(" svr=0x");
    k.putHex(s.svr, 8);
    k.puts("\r\n");
    k.puts("  Timer: ");
    k.puts(if (s.timer_enabled) "active" else "off");
    k.puts(" calibrated=");
    k.puts(if (s.timer_calibrated) "yes" else "no");
    k.puts(" one_shot=");
    k.puts(if (s.timer_one_shot) "yes" else "no");
    k.puts(" vector=0x");
    k.putHex(s.timer_vector, 2);
    k.puts(" hz=");
    k.putDec(s.timer_frequency_hz);
    k.puts(" initial=");
    k.putDec(s.timer_initial_count);
    k.puts(" current=");
    if (s.virt_base != 0) {
        k.putDec(r4os_rust_lapic_control(19, 0, 0));
    } else {
        k.putDec(s.timer_current_count);
    }
    k.puts(" ticks=");
    k.putDec(timerTickCount());
    k.puts("\r\n");
    k.puts("  Calibration: hpet_ticks=");
    k.putDec(s.calibration_hpet_ticks);
    k.puts(" lapic_ticks=");
    k.putDec(s.calibration_lapic_ticks);
    k.puts(" divide=16 lvt=0x");
    k.putHex(s.timer_lvt, 8);
    k.puts("\r\n");
    k.puts("  Timer note: ");
    k.puts(s.timer_reason);
    k.puts("\r\n");
    k.puts("  Note: ");
    k.puts(s.reason);
    k.puts("\r\n");
}

fn logStatus(s: Status) void {
    // 0.56.14: mode-Marker zusaetzlich auf COM1 (Abnahme-Kriterium);
    // der volle Status bleibt im Bootlog/DMESG.
    k.puts("[LAPIC] mode=");
    k.puts(if (s.x2apic) "x2apic" else if (s.virt_base != 0) "xapic" else "none");
    k.puts("\r\n");
    bootlog.puts("[LAPIC] mode=");
    bootlog.puts(if (s.x2apic) "x2apic" else if (s.virt_base != 0) "xapic" else "none");
    bootlog.puts(" ");
    bootlog.puts(if (s.available) "available" else "missing");
    bootlog.puts(" enabled=");
    bootlog.puts(if (s.enabled and s.software_enabled) "yes" else "no");
    bootlog.puts(" base=0x");
    bootlog.putHex(s.phys_base, 16);
    bootlog.puts(" id=");
    bootlog.putDec(s.id);
    bootlog.puts(" ver=0x");
    bootlog.putHex(s.version, 2);
    bootlog.puts(" reason=");
    bootlog.puts(s.reason);
    bootlog.puts("\r\n");
}

fn logTimerStatus(s: Status, active: bool) void {
    bootlog.puts("[LAPIC] timer=");
    bootlog.puts(if (active) "active" else "off");
    bootlog.puts(" hz=");
    bootlog.putDec(s.timer_frequency_hz);
    bootlog.puts(" initial=");
    bootlog.putDec(s.timer_initial_count);
    bootlog.puts(" hpet_ticks=");
    bootlog.putDec(s.calibration_hpet_ticks);
    bootlog.puts(" lapic_ticks=");
    bootlog.putDec(s.calibration_lapic_ticks);
    bootlog.puts(" reason=");
    bootlog.puts(s.timer_reason);
    bootlog.puts("\r\n");
}

export fn r4os_zig_lapic_log(event: u8, raw: *const RawStatus) callconv(.c) void {
    if (comptime @import("builtin").is_test) return;
    const s = project(raw.*);
    switch (event) {
        0 => logStatus(s),
        1 => logTimerStatus(s, false),
        2 => logTimerStatus(s, true),
        else => unreachable,
    }
}
