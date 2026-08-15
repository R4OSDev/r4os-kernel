const interrupts = @import("interrupts.zig");
const io = @import("io.zig");
const acpi = @import("../../platform/acpi.zig");
const bootlog = @import("../../kernel/bootlog.zig");
const k = @import("../../kernel/log.zig");
const page_tables = @import("../../memory/page_tables.zig");
const pci = @import("../../platform/pci.zig");
const pcie = @import("../../platform/pcie.zig");
const platform_cpu = @import("../../platform/cpu.zig");

const KBD_STATUS: u16 = 0x64;
const KBD_COMMAND: u16 = 0x64;
const KBD_INPUT_FULL: u8 = 0x02;
const KBD_RESET: u8 = 0xFE;
const RESET_SPACE_MEMORY: u8 = 0;
const RESET_SPACE_IO: u8 = 1;
const RESET_SPACE_PCI: u8 = 2;
const ACPI_RESET_GRACE_MILLISECONDS: u64 = 15;
const MIN_PLAUSIBLE_TSC_HZ: u64 = 1_000_000;
const MAX_PLAUSIBLE_TSC_HZ: u64 = 10_000_000_000;
const FALLBACK_TSC_HZ: u64 = 10_000_000_000;
const MAX_RESET_GRACE_SPINS: u64 = 50_000_000;

pub fn reboot() noreturn {
    io.cli();
    bootlog.puts("[RESET] reboot requested\r\n");
    // Snapshot all CPU frequency metadata before the first reset write. Once
    // firmware starts the reset, only register-local operations are trusted.
    const grace_cycles = resetGraceCycles();
    const acpi_attempted = tryAcpiReset();
    if (acpi_attempted) {
        waitAfterAcpiReset(grace_cycles);
        k.puts("[RESET] ACPI grace expired; keyboard-controller fallback\r\n");
    } else {
        k.puts("[RESET] no usable ACPI reset write; keyboard-controller fallback\r\n");
    }
    bootlog.puts("[RESET] keyboard controller fallback\r\n");
    k.serialFlush();
    var guard: u32 = 0;
    while ((io.inb(KBD_STATUS) & KBD_INPUT_FULL) != 0 and guard < 100_000) : (guard += 1) {
        io.wait();
    }
    io.outb(KBD_COMMAND, KBD_RESET);
    interrupts.haltForever();
}

fn tryAcpiReset() bool {
    const info = acpi.info();
    if (!info.fadt_reset_supported) {
        bootlog.puts("[RESET] ACPI reset not advertised\r\n");
        return false;
    }
    if (!info.fadt_reset_gas_valid) {
        bootlog.puts("[RESET][WARN] ACPI reset GAS invalid\r\n");
        k.puts("[RESET][WARN] ACPI reset GAS invalid; skipping write\r\n");
        return false;
    }
    bootlog.puts("[RESET] ACPI reset space=");
    bootlog.putDec(info.fadt_reset_address_space);
    bootlog.puts(" addr=0x");
    bootlog.putHex(info.fadt_reset_address, 16);
    bootlog.puts(" value=0x");
    bootlog.putHex(info.fadt_reset_value, 2);
    bootlog.puts("\r\n");
    k.puts("[RESET] ACPI reset attempt space=");
    k.putDec(info.fadt_reset_address_space);
    k.puts(" addr=0x");
    k.putHex(info.fadt_reset_address, 16);
    k.puts(" value=0x");
    k.putHex(info.fadt_reset_value, 2);
    k.puts(" grace_ms=15\r\n");
    k.serialFlush();
    switch (info.fadt_reset_address_space) {
        RESET_SPACE_IO => {
            if (info.fadt_reset_address <= 0xFFFF) {
                io.outb(@intCast(info.fadt_reset_address), info.fadt_reset_value);
                io.wait();
                return true;
            } else {
                bootlog.puts("[RESET][WARN] ACPI reset IO address out of range\r\n");
            }
        },
        RESET_SPACE_MEMORY => {
            return writeResetMemory(info.fadt_reset_address, info.fadt_reset_value);
        },
        RESET_SPACE_PCI => return writeResetPci(info.fadt_reset_address, info.fadt_reset_value),
        else => bootlog.puts("[RESET][WARN] unsupported ACPI reset address space\r\n"),
    }
    return false;
}

fn writeResetMemory(address: u64, value: u8) bool {
    const alias = page_tables.mmioAliasForPhysical(address, 1) orelse {
        bootlog.puts("[RESET][WARN] ACPI reset uncached MMIO alias missing\r\n");
        k.puts("[RESET][WARN] ACPI reset uncached MMIO alias missing\r\n");
        return false;
    };
    const ptr: *volatile u8 = @ptrFromInt(alias);
    ptr.* = value;
    memoryFence();
    return true;
}

fn writeResetPci(address: u64, value: u8) bool {
    const reserved_raw = address >> 48;
    const device_raw = (address >> 32) & 0xFFFF;
    const function_raw = (address >> 16) & 0xFFFF;
    const register_raw = address & 0xFFFF;
    if (reserved_raw != 0 or device_raw > 31 or function_raw > 7 or register_raw > 0x0FFF) {
        bootlog.puts("[RESET][WARN] ACPI reset PCI address out of range\r\n");
        return false;
    }

    const ecam = pcie.status();
    if (ecam.mcfg_base != 0 and ecam.segment == 0 and ecam.start_bus == 0 and register_raw <= 0x0FFF) {
        const device_base = checkedAdd(ecam.mcfg_base, device_raw << 15);
        const function_base = if (device_base) |base| checkedAdd(base, function_raw << 12) else null;
        const config_phys = if (function_base) |base| checkedAdd(base, register_raw) else null;
        if (config_phys) |phys_address| {
            if (page_tables.mmioAliasForPhysical(phys_address, 1)) |alias| {
                const ptr: *volatile u8 = @ptrFromInt(alias);
                ptr.* = value;
                memoryFence();
                io.wait();
                return true;
            }
        }
    }
    if (register_raw > 0xFF) {
        bootlog.puts("[RESET][WARN] ACPI reset PCI ECAM alias unavailable\r\n");
        return false;
    }
    pci.writeConfig8(0, @intCast(device_raw), @intCast(function_raw), @intCast(register_raw), value);
    io.wait();
    return true;
}

fn waitAfterAcpiReset(cycles: u64) void {
    // Do not touch chipset MMIO after issuing the ACPI reset write: HPET may
    // already have stopped while the CPU is still executing. TSC continues
    // in this busy loop on the supported x86_64 targets and yields a bounded
    // separation before the independent 8042 fallback.
    const start = readTsc();
    var spins: u64 = 0;
    while (readTsc() -% start < @max(cycles, 1) and spins < MAX_RESET_GRACE_SPINS) : (spins += 1) cpuRelax();
}

fn resetGraceCycles() u64 {
    const frequency = resetTscFrequencyHz();
    return (frequency * ACPI_RESET_GRACE_MILLISECONDS + 999) / 1000;
}

fn resetTscFrequencyHz() u64 {
    const info = platform_cpu.status();
    if (info.tsc_denominator != 0 and info.tsc_numerator != 0 and info.crystal_hz != 0) {
        const frequency = (@as(u128, info.crystal_hz) * info.tsc_numerator) / info.tsc_denominator;
        if (frequency >= MIN_PLAUSIBLE_TSC_HZ and frequency <= MAX_PLAUSIBLE_TSC_HZ) return @intCast(frequency);
    }
    const base_frequency = @as(u64, info.base_mhz) * 1_000_000;
    if (base_frequency >= MIN_PLAUSIBLE_TSC_HZ and base_frequency <= MAX_PLAUSIBLE_TSC_HZ) return base_frequency;
    // A deliberately high fallback keeps the grace interval at least 15 ms
    // for realistic x86_64 TSC rates while remaining finite on every path.
    return FALLBACK_TSC_HZ;
}

fn readTsc() u64 {
    var lo: u32 = undefined;
    var hi: u32 = undefined;
    asm volatile ("rdtsc"
        : [lo] "={eax}" (lo),
          [hi] "={edx}" (hi),
    );
    return (@as(u64, hi) << 32) | lo;
}

inline fn memoryFence() void {
    asm volatile ("mfence" ::: .{ .memory = true });
}

inline fn cpuRelax() void {
    asm volatile ("pause");
}

fn checkedAdd(a: u64, b: u64) ?u64 {
    if (b > 0xFFFF_FFFF_FFFF_FFFF - a) return null;
    return a + b;
}
