const acpi = @import("../../platform/acpi.zig");
const bootlog = @import("../../kernel/bootlog.zig");
const monotonic_math = @import("../../platform/monotonic_math.zig");
const k = @import("../../kernel/log.zig");
const paging = @import("../../memory/paging.zig");
const phys = @import("../../memory/phys.zig");

const REG_CAPABILITIES: u64 = 0x000;
const REG_CONFIG: u64 = 0x010;
const REG_INTERRUPT_STATUS: u64 = 0x020;
const REG_MAIN_COUNTER: u64 = 0x0F0;
const REG_TIMER0_CONFIG: u64 = 0x100;
const REG_TIMER0_COMPARATOR: u64 = 0x108;

const CONFIG_ENABLE: u64 = 1 << 0;
const CONFIG_LEGACY_REPLACEMENT: u64 = 1 << 1;
const TIMER_INT_TYPE_LEVEL: u64 = 1 << 1;
const TIMER_INT_ENABLE: u64 = 1 << 2;
const TIMER_PERIODIC: u64 = 1 << 3;
const TIMER_PERIODIC_CAPABLE: u64 = 1 << 4;
const TIMER_64BIT_CAPABLE: u64 = 1 << 5;
const TIMER_SET_ACCUMULATOR: u64 = 1 << 6;
const TIMER_32BIT_MODE: u64 = 1 << 8;
const TIMER_ROUTE_MASK: u64 = 0x1F << 9;
const TIMER_FSB_ENABLE: u64 = 1 << 14;
const FEMTOSECONDS_PER_SECOND: u64 = 1_000_000_000_000_000;

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

var current: Status = .{};
var base_virt: u64 = 0;
var counter_extension_state: u64 = 0;

pub fn initFromAcpi(info: acpi.Info) Status {
    current = .{
        .available = info.hpet_base != 0,
        .legacy_replacement_capable = info.hpet_legacy_replacement,
        .number = info.hpet_number,
        .min_tick = info.hpet_min_tick,
        .phys_base = info.hpet_base,
        .reason = "ACPI HPET missing",
    };
    base_virt = 0;
    if (!current.available) {
        logStatus();
        return current;
    }
    if (!mapMmio(current.phys_base)) {
        current.reason = "HPET MMIO mapping failed";
        logStatus();
        return current;
    }

    const cap = read64(REG_CAPABILITIES);
    current.revision = @truncate(cap & 0xFF);
    current.comparator_count = @truncate(((cap >> 8) & 0x1F) + 1);
    current.counter_64bit = ((cap >> 13) & 1) != 0;
    current.legacy_replacement_capable = ((cap >> 15) & 1) != 0;
    current.vendor_id = @truncate((cap >> 16) & 0xFFFF);
    current.period_fs = @truncate(cap >> 32);
    if (current.period_fs != 0) {
        current.frequency_hz = FEMTOSECONDS_PER_SECOND / current.period_fs;
    }

    var config = read64(REG_CONFIG);
    config |= CONFIG_ENABLE;
    config &= ~CONFIG_LEGACY_REPLACEMENT;
    write64(REG_CONFIG, config);
    current.config = read64(REG_CONFIG);
    current.enabled = (current.config & CONFIG_ENABLE) != 0;
    if (current.comparator_count > 0) {
        const timer0 = read64(REG_TIMER0_CONFIG);
        current.timer0_config = timer0;
        current.timer0_periodic_capable = (timer0 & TIMER_PERIODIC_CAPABLE) != 0;
        current.timer0_64bit_capable = (timer0 & TIMER_64BIT_CAPABLE) != 0;
    }
    current.counter = readMainCounter();
    @atomicStore(u64, &counter_extension_state, current.counter & 0xFFFF_FFFF, .release);
    current.reason = "MMIO mapped, main counter enabled, interrupts disabled";
    logStatus();
    return current;
}

pub fn status() Status {
    var s = current;
    if (s.mapped) {
        s.config = read64(REG_CONFIG);
        s.enabled = (s.config & CONFIG_ENABLE) != 0;
        s.counter = readMainCounter();
        if (s.comparator_count > 0) {
            s.timer0_config = read64(REG_TIMER0_CONFIG);
            s.timer0_comparator = read64(REG_TIMER0_COMPARATOR);
        }
    }
    return s;
}

pub fn readMainCounter() u64 {
    if (!current.mapped) return 0;
    const value = read64(REG_MAIN_COUNTER);
    if (current.counter_64bit) return value;
    return value & 0xFFFF_FFFF;
}

pub fn readExtendedMainCounter() u64 {
    if (!current.mapped) return 0;
    if (current.counter_64bit) return readMainCounter();

    var state = @atomicLoad(u64, &counter_extension_state, .acquire);
    while (true) {
        const low: u32 = @truncate(readMainCounter());
        const next = monotonic_math.extendCounter32(state, low);
        if (next == state) return state;
        if (@cmpxchgWeak(u64, &counter_extension_state, state, next, .acq_rel, .acquire)) |actual| {
            state = actual;
        } else {
            return next;
        }
    }
}

pub fn elapsedMainCounter(start: u64, end: u64) u64 {
    if (current.counter_64bit) return end -% start;
    return (end -% start) & 0xFFFF_FFFF;
}

pub fn startLegacyIrqTimer(requested_hz: u32) bool {
    if (!current.mapped or current.frequency_hz == 0 or current.comparator_count == 0) {
        current.reason = "HPET timer unavailable";
        return false;
    }
    if (!current.legacy_replacement_capable) {
        current.reason = "HPET legacy replacement not supported";
        return false;
    }
    if (!current.timer0_periodic_capable) {
        current.reason = "HPET comparator 0 is not periodic capable";
        return false;
    }

    const hz = if (requested_hz == 0) 100 else requested_hz;
    var period = current.frequency_hz / hz;
    if (period == 0) period = 1;
    current.timer_hz = hz;
    current.timer0_period = period;
    current.timer0_one_shot = false;

    var timer_config = read64(REG_TIMER0_CONFIG);
    timer_config &= ~(TIMER_INT_TYPE_LEVEL | TIMER_INT_ENABLE | TIMER_PERIODIC | TIMER_32BIT_MODE | TIMER_ROUTE_MASK | TIMER_FSB_ENABLE);
    timer_config |= TIMER_PERIODIC | TIMER_SET_ACCUMULATOR;
    write64(REG_TIMER0_CONFIG, timer_config);
    const counter_mask: u64 = if (current.timer0_64bit_capable) ~@as(u64, 0) else 0xFFFF_FFFF;
    const first_deadline = (readMainCounter() +% period) & counter_mask;
    write64(REG_TIMER0_COMPARATOR, first_deadline);
    write64(REG_TIMER0_COMPARATOR, period);
    timer_config |= TIMER_INT_ENABLE;
    write64(REG_TIMER0_CONFIG, timer_config);

    const config = read64(REG_CONFIG) | CONFIG_ENABLE | CONFIG_LEGACY_REPLACEMENT;
    write64(REG_CONFIG, config);

    current.config = read64(REG_CONFIG);
    current.timer0_config = read64(REG_TIMER0_CONFIG);
    current.timer0_comparator = read64(REG_TIMER0_COMPARATOR);
    current.counter = readMainCounter();
    current.enabled = (current.config & CONFIG_ENABLE) != 0;
    current.timer0_irq_active = current.enabled and (current.config & CONFIG_LEGACY_REPLACEMENT) != 0 and (current.timer0_config & TIMER_INT_ENABLE) != 0;
    current.reason = if (current.timer0_irq_active)
        "HPET comparator 0 periodic legacy IRQ0 active"
    else
        "HPET comparator 0 setup incomplete";
    logStatus();
    return current.timer0_irq_active;
}

pub fn startLegacyOneShotTimer(tick_hz: u32) bool {
    if (!current.mapped or current.frequency_hz == 0 or current.comparator_count == 0) {
        current.reason = "HPET one-shot unavailable";
        return false;
    }
    if (!current.legacy_replacement_capable) {
        current.reason = "HPET one-shot needs legacy IRQ0 replacement";
        return false;
    }

    var timer_config = read64(REG_TIMER0_CONFIG);
    timer_config &= ~(TIMER_INT_TYPE_LEVEL | TIMER_INT_ENABLE | TIMER_PERIODIC | TIMER_SET_ACCUMULATOR | TIMER_32BIT_MODE | TIMER_ROUTE_MASK | TIMER_FSB_ENABLE);
    write64(REG_TIMER0_CONFIG, timer_config);
    write64(REG_INTERRUPT_STATUS, 1);

    const config = read64(REG_CONFIG) | CONFIG_ENABLE | CONFIG_LEGACY_REPLACEMENT;
    write64(REG_CONFIG, config);

    current.config = read64(REG_CONFIG);
    current.timer0_config = read64(REG_TIMER0_CONFIG);
    current.enabled = (current.config & CONFIG_ENABLE) != 0;
    current.timer0_irq_active = false;
    current.timer0_one_shot = current.enabled and (current.config & CONFIG_LEGACY_REPLACEMENT) != 0;
    current.timer_hz = tick_hz;
    current.timer0_period = 0;
    current.reason = if (current.timer0_one_shot)
        "HPET comparator 0 one-shot legacy IRQ0 ready"
    else
        "HPET one-shot setup incomplete";
    return current.timer0_one_shot;
}

pub fn armLegacyOneShotTicks(requested_ticks: u64, tick_hz: u32) u64 {
    if (!current.timer0_one_shot and !startLegacyOneShotTimer(tick_hz)) return 0;
    if (tick_hz == 0 or current.frequency_hz == 0) return 0;

    const wanted_ticks = @max(@as(u64, 1), requested_ticks);
    const counter_limit: u64 = if (current.timer0_64bit_capable)
        0x3FFF_FFFF_FFFF_FFFF
    else
        0x7FFF_FFFF;
    var counter_delta = monotonic_math.ticksToCounterCeil(wanted_ticks, current.frequency_hz, tick_hz);
    counter_delta = @max(counter_delta, @as(u64, @max(current.min_tick, 1)));
    const clamped = counter_delta > counter_limit;
    if (clamped) counter_delta = counter_limit;

    var timer_config = read64(REG_TIMER0_CONFIG);
    timer_config &= ~(TIMER_INT_TYPE_LEVEL | TIMER_INT_ENABLE | TIMER_PERIODIC | TIMER_SET_ACCUMULATOR | TIMER_32BIT_MODE | TIMER_ROUTE_MASK | TIMER_FSB_ENABLE);
    write64(REG_TIMER0_CONFIG, timer_config);
    write64(REG_INTERRUPT_STATUS, 1);

    const counter_mask: u64 = if (current.timer0_64bit_capable) ~@as(u64, 0) else 0xFFFF_FFFF;
    const comparator = (readMainCounter() +% counter_delta) & counter_mask;
    write64(REG_TIMER0_COMPARATOR, comparator);
    timer_config |= TIMER_INT_ENABLE;
    write64(REG_TIMER0_CONFIG, timer_config);

    current.timer0_config = read64(REG_TIMER0_CONFIG);
    current.timer0_comparator = comparator;
    current.timer0_irq_active = (current.timer0_config & TIMER_INT_ENABLE) != 0;
    if (!current.timer0_irq_active) return 0;
    current.reason = "HPET comparator 0 one-shot armed";
    return if (clamped)
        @max(@as(u64, 1), monotonic_math.counterToTicks(counter_delta, current.frequency_hz, tick_hz))
    else
        wanted_ticks;
}

pub fn disarmLegacyOneShotTimer() void {
    if (!current.mapped or !current.timer0_one_shot) return;
    var timer_config = read64(REG_TIMER0_CONFIG);
    timer_config &= ~TIMER_INT_ENABLE;
    write64(REG_TIMER0_CONFIG, timer_config);
    write64(REG_INTERRUPT_STATUS, 1);
    current.timer0_config = read64(REG_TIMER0_CONFIG);
    current.timer0_irq_active = false;
    current.reason = "HPET comparator 0 one-shot disarmed";
}

pub fn stopLegacyIrqTimer() void {
    if (!current.mapped) return;
    var timer_config = read64(REG_TIMER0_CONFIG);
    timer_config &= ~(TIMER_INT_ENABLE | TIMER_PERIODIC | TIMER_SET_ACCUMULATOR);
    write64(REG_TIMER0_CONFIG, timer_config);

    var config = read64(REG_CONFIG);
    config &= ~CONFIG_LEGACY_REPLACEMENT;
    config |= CONFIG_ENABLE;
    write64(REG_CONFIG, config);

    current.config = read64(REG_CONFIG);
    current.timer0_config = read64(REG_TIMER0_CONFIG);
    current.timer0_comparator = read64(REG_TIMER0_COMPARATOR);
    current.enabled = (current.config & CONFIG_ENABLE) != 0;
    current.timer0_irq_active = false;
    current.timer0_one_shot = false;
    current.timer_hz = 0;
    current.timer0_period = 0;
    current.reason = "HPET main counter enabled, comparator interrupts disabled";
    logStatus();
}

pub fn onTimerIrq() u64 {
    if (current.mapped) write64(REG_INTERRUPT_STATUS, 1);
    const p: *volatile u64 = &current.timer_ticks;
    p.* +%= 1;
    return p.*;
}

pub fn timerTickCount() u64 {
    const p: *volatile u64 = &current.timer_ticks;
    return p.*;
}

pub fn timerFrequency() u32 {
    return current.timer_hz;
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

fn mapMmio(base_phys: u64) bool {
    const page = base_phys & ~(paging.PAGE_SIZE - 1);
    const virt_page = phys.physToVirt(page);
    if (!paging.isMapped(virt_page)) {
        if (!paging.mapPage(virt_page, page, paging.WRITABLE | paging.CACHE_DISABLE | paging.NO_EXECUTE)) {
            return false;
        }
    }
    base_virt = phys.physToVirt(base_phys);
    current.virt_base = base_virt;
    current.mapped = true;
    return true;
}

fn read64(offset: u64) u64 {
    const ptr: *volatile u64 = @ptrFromInt(base_virt + offset);
    return ptr.*;
}

fn write64(offset: u64, value: u64) void {
    const ptr: *volatile u64 = @ptrFromInt(base_virt + offset);
    ptr.* = value;
}

fn logStatus() void {
    bootlog.puts("[HPET] ");
    bootlog.puts(if (current.available) "available" else "missing");
    bootlog.puts(" mapped=");
    bootlog.puts(if (current.mapped) "yes" else "no");
    bootlog.puts(" enabled=");
    bootlog.puts(if (current.enabled) "yes" else "no");
    bootlog.puts(" base=0x");
    bootlog.putHex(current.phys_base, 16);
    bootlog.puts(" period_fs=");
    bootlog.putDec(current.period_fs);
    bootlog.puts(" freq_hz=");
    bootlog.putDec(current.frequency_hz);
    bootlog.puts(" counter=");
    bootlog.putDec(current.counter);
    bootlog.puts(" timer0_irq=");
    bootlog.puts(if (current.timer0_irq_active) "on" else "off");
    bootlog.puts(" timer_hz=");
    bootlog.putDec(current.timer_hz);
    bootlog.puts(" timer_ticks=");
    bootlog.putDec(current.timer_ticks);
    bootlog.puts(" reason=");
    bootlog.puts(current.reason);
    bootlog.puts("\r\n");
}
