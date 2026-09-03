const acpi = @import("../../platform/acpi.zig");
const bootlog = @import("../../kernel/bootlog.zig");
const hpet = @import("hpet.zig");
const k = @import("../../kernel/log.zig");
const msr = @import("msr.zig");
const config = @import("config");
const platform_cpu = @import("../../platform/cpu.zig");
const paging = @import("../../memory/paging.zig");
const phys = @import("../../memory/phys.zig");

const IA32_APIC_BASE: u32 = 0x1B;
const APIC_BASE_BSP: u64 = 1 << 8;
const APIC_BASE_X2APIC: u64 = 1 << 10;
const APIC_BASE_ENABLE: u64 = 1 << 11;
const APIC_BASE_MASK: u64 = 0x000F_FFFF_FFFF_F000;

const REG_ID: u32 = 0x020;
const REG_VERSION: u32 = 0x030;
const REG_EOI: u32 = 0x0B0;
const REG_SVR: u32 = 0x0F0;
const REG_ICR_LOW: u32 = 0x300;
const REG_ICR_HIGH: u32 = 0x310;
const REG_LVT_TIMER: u32 = 0x320;
const REG_TIMER_INITIAL_COUNT: u32 = 0x380;
const REG_TIMER_CURRENT_COUNT: u32 = 0x390;
const REG_TIMER_DIVIDE: u32 = 0x3E0;

const SPURIOUS_VECTOR: u32 = 0xFF;
const SVR_ENABLE: u32 = 1 << 8;
const TIMER_VECTOR: u8 = 0x20;
const TIMER_MASKED: u32 = 1 << 16;
const TIMER_PERIODIC: u32 = 1 << 17;
const TIMER_DIVIDE_BY_16: u32 = 0x3;
const CALIBRATION_HPET_DIVISOR: u64 = 100;
const MIN_TIMER_INITIAL_COUNT: u32 = 16;
const ICR_DELIVERY_PENDING: u32 = 1 << 12;
const ICR_INIT_ASSERT: u32 = 0x0000_C500;
const ICR_INIT_DEASSERT: u32 = 0x0000_8500;
const ICR_STARTUP: u32 = 0x0000_0600;

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

var current: Status = .{};
var base_virt: u64 = 0;
// 0.56.14: x2APIC-MSR-Modus. Registerzugriffe laufen dann ueber
// MSR 0x800 + (MMIO-Offset >> 4) statt ueber das MMIO-Fenster.
var x2_mode: bool = false;
const X2APIC_MSR_BASE: u32 = 0x800;

pub fn initFromAcpi(info: acpi.Info) Status {
    current = .{
        .available = info.madt_lapic_address != 0 and info.madt_lapic_enabled_count > 0,
        .phys_base = info.madt_lapic_address,
        .reason = "MADT has no enabled LAPIC",
        .timer_reason = "timer not initialized",
    };
    base_virt = 0;
    if (!current.available) {
        logStatus();
        return current;
    }

    const apic_base = msr.read(IA32_APIC_BASE);
    current.msr_base = apic_base & APIC_BASE_MASK;
    current.bsp = (apic_base & APIC_BASE_BSP) != 0;
    current.x2apic = (apic_base & APIC_BASE_X2APIC) != 0;
    current.enabled = (apic_base & APIC_BASE_ENABLE) != 0;
    if (current.phys_base == 0) current.phys_base = current.msr_base;

    // 0.56.14: Opt-in-Forcierung (-Dforce-x2apic, Default aus) - nur
    // wenn die CPU x2APIC meldet (CPUID.1:ECX.21), sonst wuerde das
    // EXTD-Bit einen #GP ausloesen.
    if (config.force_x2apic and !current.x2apic and platform_cpu.status().features.x2apic) {
        msr.write(IA32_APIC_BASE, apic_base | APIC_BASE_ENABLE | APIC_BASE_X2APIC);
        const forced = msr.read(IA32_APIC_BASE);
        current.x2apic = (forced & APIC_BASE_X2APIC) != 0;
        current.enabled = (forced & APIC_BASE_ENABLE) != 0;
    }

    if (current.x2apic) {
        // 0.56.14: vollwertiger MSR-Pfad statt Bail-out. Kein MMIO-
        // Mapping noetig; dieselbe Init-Sequenz laeuft ueber MSRs.
        x2_mode = true;
        if (!current.enabled) {
            msr.write(IA32_APIC_BASE, msr.read(IA32_APIC_BASE) | APIC_BASE_ENABLE | APIC_BASE_X2APIC);
            current.enabled = true;
        }
        // ID ist im x2APIC-Modus das volle 32-Bit-Register.
        current.id = @truncate(readReg(REG_ID) & 0xFF);
        const version_reg_x2 = readReg(REG_VERSION);
        current.version = @truncate(version_reg_x2);
        current.max_lvt = @truncate((version_reg_x2 >> 16) & 0xFF);
        const old_svr_x2 = readReg(REG_SVR);
        writeReg(REG_SVR, (old_svr_x2 & ~@as(u32, 0xFF)) | SVR_ENABLE | SPURIOUS_VECTOR);
        current.svr = readReg(REG_SVR);
        current.software_enabled = (current.svr & SVR_ENABLE) != 0;
        current.reason = "x2APIC MSR mode active, PIC/IOAPIC routing unchanged";
        logStatus();
        return current;
    }
    if (current.phys_base == 0) {
        current.reason = "LAPIC base missing";
        logStatus();
        return current;
    }
    if (!mapMmio(current.phys_base)) {
        current.reason = "LAPIC MMIO mapping failed";
        logStatus();
        return current;
    }

    if (!current.enabled) {
        msr.write(IA32_APIC_BASE, apic_base | APIC_BASE_ENABLE);
        current.enabled = true;
    }

    const id_reg = readReg(REG_ID);
    const version_reg = readReg(REG_VERSION);
    current.id = @truncate(id_reg >> 24);
    current.version = @truncate(version_reg);
    current.max_lvt = @truncate((version_reg >> 16) & 0xFF);

    const old_svr = readReg(REG_SVR);
    writeReg(REG_SVR, (old_svr & ~@as(u32, 0xFF)) | SVR_ENABLE | SPURIOUS_VECTOR);
    current.svr = readReg(REG_SVR);
    current.software_enabled = (current.svr & SVR_ENABLE) != 0;
    current.reason = "xAPIC MMIO enabled, PIC/IOAPIC routing unchanged";

    logStatus();
    return current;
}

pub fn status() Status {
    return current;
}

pub fn isEnabled() bool {
    return current.enabled and current.software_enabled and (base_virt != 0 or x2_mode);
}

pub fn initCurrentCpu() bool {
    if (base_virt == 0 and !x2_mode) return false;
    var apic_base = msr.read(IA32_APIC_BASE);
    apic_base |= APIC_BASE_ENABLE;
    if (x2_mode) apic_base |= APIC_BASE_X2APIC;
    msr.write(IA32_APIC_BASE, apic_base);
    const old_svr = readReg(REG_SVR);
    writeReg(REG_SVR, (old_svr & ~@as(u32, 0xFF)) | SVR_ENABLE | SPURIOUS_VECTOR);
    return (readReg(REG_SVR) & SVR_ENABLE) != 0;
}

pub fn localApicId() u32 {
    if (x2_mode) return readReg(REG_ID);
    return readReg(REG_ID) >> 24;
}

// AP timers are calibrated independently because the global event clock may
// use HPET and LAPIC calibration is CPU-local.  Only the resulting count is
// stored in CpuLocal; BSP timer/deadline state remains untouched.
pub fn startSecondaryPeriodicTimer(requested_hz: u32) bool {
    if (!initCurrentCpu()) return false;
    const reference = hpet.status();
    if (!reference.enabled or reference.frequency_hz == 0) return false;
    const hz = if (requested_hz == 0) 100 else requested_hz;
    const hpet_delta = reference.frequency_hz / CALIBRATION_HPET_DIVISOR;
    if (hpet_delta == 0) return false;

    writeReg(REG_TIMER_INITIAL_COUNT, 0);
    writeReg(REG_TIMER_DIVIDE, TIMER_DIVIDE_BY_16);
    writeReg(REG_LVT_TIMER, TIMER_VECTOR | TIMER_MASKED);
    writeReg(REG_TIMER_INITIAL_COUNT, 0xFFFF_FFFF);
    const start_hpet = hpet.readMainCounter();
    while (hpet.elapsedMainCounter(start_hpet, hpet.readMainCounter()) < hpet_delta) {
        asm volatile ("pause");
    }
    const elapsed_hpet = hpet.elapsedMainCounter(start_hpet, hpet.readMainCounter());
    const elapsed_lapic = 0xFFFF_FFFF -% readReg(REG_TIMER_CURRENT_COUNT);
    writeReg(REG_TIMER_INITIAL_COUNT, 0);
    if (elapsed_hpet == 0 or elapsed_lapic < MIN_TIMER_INITIAL_COUNT) return false;
    const numerator = @as(u128, elapsed_lapic) * @as(u128, reference.frequency_hz);
    const denominator = @as(u128, elapsed_hpet) * @as(u128, hz);
    if (denominator == 0) return false;
    const initial = numerator / denominator;
    if (initial < MIN_TIMER_INITIAL_COUNT or initial > 0xFFFF_FFFE) return false;
    writeReg(REG_TIMER_DIVIDE, TIMER_DIVIDE_BY_16);
    writeReg(REG_LVT_TIMER, TIMER_VECTOR | TIMER_PERIODIC);
    writeReg(REG_TIMER_INITIAL_COUNT, @intCast(initial));
    return true;
}

pub fn sendInitSipi(apic_id: u32, vector: u8) bool {
    if (!isEnabled() or vector == 0) return false;
    if (!writeIcr(apic_id, ICR_INIT_ASSERT) or !waitIcrIdle()) return false;
    delayMicroseconds(10_000);
    if (!writeIcr(apic_id, ICR_INIT_DEASSERT) or !waitIcrIdle()) return false;
    delayMicroseconds(200);
    if (!writeIcr(apic_id, ICR_STARTUP | vector) or !waitIcrIdle()) return false;
    delayMicroseconds(200);
    if (!writeIcr(apic_id, ICR_STARTUP | vector) or !waitIcrIdle()) return false;
    return true;
}

pub fn sendReschedule(apic_id: u32, vector: u8) bool {
    return writeIcr(apic_id, vector) and waitIcrIdle();
}

pub fn sendStop(apic_id: u32, vector: u8) bool {
    return writeIcr(apic_id, vector) and waitIcrIdle();
}

pub fn sendIpi(apic_id: u32, vector: u8) bool {
    if (!isEnabled() or vector < 0x20) return false;
    return writeIcr(apic_id, vector) and waitIcrIdle();
}

pub fn initTimerFromHpet(requested_hz: u32) bool {
    if (!isEnabled()) {
        current.timer_reason = "LAPIC not enabled";
        logTimerStatus(false);
        return false;
    }
    const hpet_status = hpet.status();
    if (!hpet_status.enabled or hpet_status.frequency_hz == 0) {
        current.timer_reason = "HPET reference clock unavailable";
        logTimerStatus(false);
        return false;
    }
    const hz = if (requested_hz == 0) 100 else requested_hz;
    const hpet_delta = hpet_status.frequency_hz / CALIBRATION_HPET_DIVISOR;
    if (hpet_delta == 0) {
        current.timer_reason = "HPET reference too slow";
        logTimerStatus(false);
        return false;
    }

    stopTimer();
    writeReg(REG_TIMER_DIVIDE, TIMER_DIVIDE_BY_16);
    writeReg(REG_LVT_TIMER, TIMER_VECTOR | TIMER_MASKED);
    writeReg(REG_TIMER_INITIAL_COUNT, 0xFFFF_FFFF);

    const start_hpet = hpet.readMainCounter();
    while (hpet.elapsedMainCounter(start_hpet, hpet.readMainCounter()) < hpet_delta) {
        asm volatile ("pause");
    }
    const elapsed_hpet = hpet.elapsedMainCounter(start_hpet, hpet.readMainCounter());
    const lapic_current = readReg(REG_TIMER_CURRENT_COUNT);
    const elapsed_lapic = 0xFFFF_FFFF -% lapic_current;
    stopTimer();

    if (elapsed_hpet == 0 or elapsed_lapic < MIN_TIMER_INITIAL_COUNT) {
        current.timer_reason = "LAPIC timer did not count during calibration";
        logTimerStatus(false);
        return false;
    }

    const numerator = @as(u128, elapsed_lapic) * @as(u128, hpet_status.frequency_hz);
    const denominator = @as(u128, elapsed_hpet) * @as(u128, hz);
    if (denominator == 0) {
        current.timer_reason = "LAPIC calibration denominator is zero";
        logTimerStatus(false);
        return false;
    }
    const initial_u128 = numerator / denominator;
    if (initial_u128 < MIN_TIMER_INITIAL_COUNT or initial_u128 > 0xFFFF_FFFE) {
        current.timer_reason = "LAPIC calibrated count out of range";
        logTimerStatus(false);
        return false;
    }

    current.timer_frequency_hz = hz;
    current.timer_initial_count = @intCast(initial_u128);
    current.calibration_hpet_ticks = elapsed_hpet;
    current.calibration_lapic_ticks = elapsed_lapic;
    current.timer_calibrated = true;
    current.timer_reason = "calibrated against HPET";

    writeReg(REG_TIMER_DIVIDE, TIMER_DIVIDE_BY_16);
    writeReg(REG_LVT_TIMER, TIMER_VECTOR | TIMER_PERIODIC);
    writeReg(REG_TIMER_INITIAL_COUNT, current.timer_initial_count);
    current.timer_lvt = readReg(REG_LVT_TIMER);
    current.timer_current_count = readReg(REG_TIMER_CURRENT_COUNT);
    current.timer_enabled = true;
    current.timer_one_shot = false;
    current.timer_reason = "periodic LAPIC timer active";
    logTimerStatus(true);
    return true;
}

pub fn startOneShotTimer() bool {
    if (!isEnabled() or !current.timer_calibrated or current.timer_initial_count < MIN_TIMER_INITIAL_COUNT) {
        current.timer_reason = "LAPIC one-shot unavailable without calibration";
        return false;
    }
    writeReg(REG_TIMER_DIVIDE, TIMER_DIVIDE_BY_16);
    writeReg(REG_TIMER_INITIAL_COUNT, 0);
    writeReg(REG_LVT_TIMER, TIMER_VECTOR | TIMER_MASKED);
    current.timer_lvt = readReg(REG_LVT_TIMER);
    current.timer_current_count = readReg(REG_TIMER_CURRENT_COUNT);
    current.timer_enabled = true;
    current.timer_one_shot = true;
    current.timer_reason = "LAPIC one-shot ready";
    return true;
}

pub fn armOneShotTicks(requested_ticks: u64) u64 {
    if (!current.timer_one_shot and !startOneShotTimer()) return 0;
    const count_per_tick = @as(u64, current.timer_initial_count);
    if (count_per_tick == 0) return 0;
    const maximum_ticks = @max(@as(u64, 1), @as(u64, 0xFFFF_FFFE) / count_per_tick);
    const programmed_ticks = @min(@max(@as(u64, 1), requested_ticks), maximum_ticks);
    const initial_count: u32 = @intCast(programmed_ticks * count_per_tick);

    writeReg(REG_TIMER_DIVIDE, TIMER_DIVIDE_BY_16);
    writeReg(REG_LVT_TIMER, TIMER_VECTOR);
    writeReg(REG_TIMER_INITIAL_COUNT, initial_count);
    current.timer_lvt = readReg(REG_LVT_TIMER);
    current.timer_current_count = readReg(REG_TIMER_CURRENT_COUNT);
    current.timer_enabled = true;
    current.timer_reason = "LAPIC one-shot armed";
    return programmed_ticks;
}

pub fn disarmOneShotTimer() void {
    if (!current.timer_one_shot) return;
    writeReg(REG_TIMER_INITIAL_COUNT, 0);
    writeReg(REG_LVT_TIMER, TIMER_VECTOR | TIMER_MASKED);
    current.timer_lvt = readReg(REG_LVT_TIMER);
    current.timer_current_count = readReg(REG_TIMER_CURRENT_COUNT);
    current.timer_reason = "LAPIC one-shot disarmed";
}

pub fn resumePeriodicTimer() bool {
    if (!isEnabled() or !current.timer_calibrated or current.timer_initial_count < MIN_TIMER_INITIAL_COUNT) {
        current.timer_reason = "LAPIC periodic resume unavailable";
        return false;
    }
    writeReg(REG_TIMER_DIVIDE, TIMER_DIVIDE_BY_16);
    writeReg(REG_LVT_TIMER, TIMER_VECTOR | TIMER_PERIODIC);
    writeReg(REG_TIMER_INITIAL_COUNT, current.timer_initial_count);
    current.timer_lvt = readReg(REG_LVT_TIMER);
    current.timer_current_count = readReg(REG_TIMER_CURRENT_COUNT);
    current.timer_enabled = true;
    current.timer_one_shot = false;
    current.timer_reason = "periodic LAPIC timer active";
    return true;
}

pub fn stopTimer() void {
    if (base_virt != 0 or x2_mode) {
        writeReg(REG_TIMER_INITIAL_COUNT, 0);
        writeReg(REG_LVT_TIMER, TIMER_VECTOR | TIMER_MASKED);
        current.timer_lvt = readReg(REG_LVT_TIMER);
        current.timer_current_count = readReg(REG_TIMER_CURRENT_COUNT);
    }
    current.timer_enabled = false;
    current.timer_one_shot = false;
}

pub fn onTimerIrq() u64 {
    const p: *volatile u64 = &current.timer_ticks;
    p.* +%= 1;
    current.timer_current_count = readReg(REG_TIMER_CURRENT_COUNT);
    return p.*;
}

pub fn timerTickCount() u64 {
    const p: *volatile u64 = &current.timer_ticks;
    return p.*;
}

pub fn timerFrequency() u32 {
    return current.timer_frequency_hz;
}

pub fn isTimerActive() bool {
    return current.timer_enabled and current.timer_calibrated and current.timer_initial_count != 0;
}

pub fn endOfInterrupt() void {
    if (isEnabled()) writeReg(REG_EOI, 0);
}

pub fn dumpStatus() void {
    const s = current;
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
    k.putHex(base_virt, 16);
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
    if (base_virt != 0) {
        k.putDec(readReg(REG_TIMER_CURRENT_COUNT));
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

fn mapMmio(base_phys: u64) bool {
    const page = base_phys & ~(paging.PAGE_SIZE - 1);
    const virt_page = phys.physToVirt(page);
    if (!paging.isMapped(virt_page)) {
        if (!paging.mapPage(virt_page, page, paging.WRITABLE | paging.CACHE_DISABLE | paging.NO_EXECUTE)) {
            return false;
        }
    }
    base_virt = phys.physToVirt(base_phys);
    return true;
}

fn readReg(offset: u32) u32 {
    if (x2_mode) {
        return @truncate(msr.read(X2APIC_MSR_BASE + (offset >> 4)));
    }
    const ptr: *volatile u32 = @ptrFromInt(base_virt + offset);
    return ptr.*;
}

fn writeReg(offset: u32, value: u32) void {
    if (x2_mode) {
        msr.write(X2APIC_MSR_BASE + (offset >> 4), value);
        return;
    }
    const ptr: *volatile u32 = @ptrFromInt(base_virt + offset);
    ptr.* = value;
}

fn writeIcr(apic_id: u32, low: u32) bool {
    if (x2_mode) {
        msr.write(X2APIC_MSR_BASE + (REG_ICR_LOW >> 4), (@as(u64, apic_id) << 32) | low);
        return true;
    }
    if (apic_id > 0xFF or base_virt == 0) return false;
    writeReg(REG_ICR_HIGH, apic_id << 24);
    writeReg(REG_ICR_LOW, low);
    return true;
}

fn waitIcrIdle() bool {
    var spins: u32 = 0;
    while ((readReg(REG_ICR_LOW) & ICR_DELIVERY_PENDING) != 0) : (spins += 1) {
        if (spins >= 1_000_000) return false;
        asm volatile ("pause");
    }
    return true;
}

fn delayMicroseconds(microseconds: u64) void {
    const reference = hpet.status();
    if (reference.enabled and reference.frequency_hz != 0) {
        const requested = (@as(u128, reference.frequency_hz) * microseconds) / 1_000_000;
        const delta: u64 = @intCast(@max(@as(u128, 1), requested));
        const start = hpet.readMainCounter();
        while (hpet.elapsedMainCounter(start, hpet.readMainCounter()) < delta) asm volatile ("pause");
        return;
    }
    var spins = microseconds *| 256;
    while (spins != 0) : (spins -= 1) asm volatile ("pause");
}

fn logStatus() void {
    // 0.56.14: mode-Marker zusaetzlich auf COM1 (Abnahme-Kriterium);
    // der volle Status bleibt im Bootlog/DMESG.
    k.puts("[LAPIC] mode=");
    k.puts(if (x2_mode) "x2apic" else if (base_virt != 0) "xapic" else "none");
    k.puts("\r\n");
    bootlog.puts("[LAPIC] mode=");
    bootlog.puts(if (x2_mode) "x2apic" else if (base_virt != 0) "xapic" else "none");
    bootlog.puts(" ");
    bootlog.puts(if (current.available) "available" else "missing");
    bootlog.puts(" enabled=");
    bootlog.puts(if (current.enabled and current.software_enabled) "yes" else "no");
    bootlog.puts(" base=0x");
    bootlog.putHex(current.phys_base, 16);
    bootlog.puts(" id=");
    bootlog.putDec(current.id);
    bootlog.puts(" ver=0x");
    bootlog.putHex(current.version, 2);
    bootlog.puts(" reason=");
    bootlog.puts(current.reason);
    bootlog.puts("\r\n");
}

fn logTimerStatus(active: bool) void {
    bootlog.puts("[LAPIC] timer=");
    bootlog.puts(if (active) "active" else "off");
    bootlog.puts(" hz=");
    bootlog.putDec(current.timer_frequency_hz);
    bootlog.puts(" initial=");
    bootlog.putDec(current.timer_initial_count);
    bootlog.puts(" hpet_ticks=");
    bootlog.putDec(current.calibration_hpet_ticks);
    bootlog.puts(" lapic_ticks=");
    bootlog.putDec(current.calibration_lapic_ticks);
    bootlog.puts(" reason=");
    bootlog.puts(current.timer_reason);
    bootlog.puts("\r\n");
}
